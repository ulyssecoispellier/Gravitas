#version 450

// Ray-tracing pass: runs once per pixel of the screen

// Position on screen, -1..1 (from raytrace.vert)
layout(location = 0) in vec2 screen_pos;

layout(location = 0) out vec4 out_color;

// Same layout as `RayPushConstants` in renderer.rs
layout(push_constant) uniform Push {
    mat4 view_proj;
    vec4 camera_pos; // xyz = camera, w = frame number
    vec4 sun;      // xyz = centre, w = radius
} push;

layout(set = 0, binding = 1) uniform Bodies {
    vec4 bodies[16];
    vec4 colours[16];
    vec4 atmospheres[16]; // rgb = scattering strength per colour (β), w = outer radius ÷ planet radius (0 = none)
    vec4 haze[16];        // rgb = Mie β (dust, droplets), w = forward bias g
} all_bodies;

layout(set = 0, binding = 2) uniform sampler2D sky;
layout(set = 0, binding = 0) uniform sampler2D planet_texture;

// How bright the Sun is compared with a lit planet (HDR: can be much more than 1.0)
const float SUN_INTENSITY = 4.0;
const float PI = 3.14159265;
const int VIEW_STEPS = 32; // samples along the view ray inside an atmosphere
const int LIGHT_STEPS = 8; // samples along each sample's ray towards the Sun
const int BOUNCE_SAMPLES = 8; // aimed rays per nearby body, for the bounce light

// Distance along the ray to the nearest hit, or -1.0 if it misses (same as ray.rs)
float ray_sphere(vec3 origin, vec3 dir, vec3 center, float radius) {
    vec3 ctr_to_origin = origin - center;
    float projection = dot(dir, ctr_to_origin);
    float gap = dot(ctr_to_origin, ctr_to_origin) - radius * radius;
    float discriminant = projection * projection - gap;

    if (discriminant < 0.0) {
        return -1.0;
    }

    float dist = -projection - sqrt(discriminant);

    if (dist < 0.0) { return -1.0; } else { return dist; }
}

// Entry and exit distances along the ray (x = in, y = out), or vec2(-1.0) if it misses
vec2 ray_sphere_both(vec3 origin, vec3 dir, vec3 center, float radius) {
    vec3 ctr_to_origin = origin - center;
    float projection = dot(dir, ctr_to_origin);
    float gap = dot(ctr_to_origin, ctr_to_origin) - radius * radius;
    float discriminant = projection * projection - gap;

    if (discriminant < 0.0) {
        return vec2(-1.0, -1.0);
    }

    float s = sqrt(discriminant);

    return vec2(-projection - s, -projection + s);
}

// Fraction of the Sun visible from `point` (1 = all, 0 = fully eclipsed), soft cones from every body but `skip`
float sun_visibility(vec3 point, int skip) {
    vec3 light_dir = normalize(push.sun.xyz - point);
    float sun_dist = length(push.sun.xyz - point);
    float vis = 1.0;

    for (int i = 1; i < 16; i++) {   // from 1: body 0 is the Sun itself
        vec4 other = all_bodies.bodies[i];

        if (other.w == 0.0) { continue; }
        if (i == skip) { continue; }

        vec3 to_blocker = other.xyz - point;
        float blocker_dist = length(to_blocker);
        if (blocker_dist > sun_dist) { continue; }           // beyond the Sun: can't shade us

        float a_sun = asin(clamp(all_bodies.bodies[0].w / sun_dist, 0.0, 1.0)); // body 0 = the Sun
        float a_b   = asin(clamp(other.w / blocker_dist, 0.0, 1.0));
        float theta = acos(clamp(dot(normalize(to_blocker), light_dir), -1.0, 1.0));

        // How much of the Sun is hidden: 0 = none, up to 1 = all of it
        float max_cover = min(1.0, (a_b * a_b) / (a_sun * a_sun));
        float cover = (1.0 - smoothstep(abs(a_sun - a_b), a_sun + a_b, theta)) * max_cover;
        vis *= 1.0 - cover;
    }

    return vis;
}

// Surface colour of body `index` at `point`: its texture where it has one (colours.w = 1), else its base colour
vec3 albedo(int index, vec3 point) {
    vec3 normal = normalize(point - all_bodies.bodies[index].xyz);
    float pu = atan(normal.y, normal.x) / (2.0 * PI);
    float pv = acos(normal.z) / PI;
    return mix(all_bodies.colours[index].rgb, texture(planet_texture, vec2(pu, pv)).rgb, all_bodies.colours[index].w);
}

// Light that `point` on body `index` reflects from the Sun alone: albedo × facing × eclipses
vec3 direct_light(int index, vec3 point) {
    vec3 normal = normalize(point - all_bodies.bodies[index].xyz);
    vec3 light_dir = normalize(push.sun.xyz - point);
    float brightness = max(dot(normal, light_dir), 0.0);

    brightness *= sun_visibility(point, index);

    return albedo(index, point) * brightness;
}

// Random direction above a surface (cosine-weighted: near the normal more often), from two numbers in [0, 1)
vec3 random_hemisphere(vec3 normal, float r1, float r2) {
    // Direction above a flat floor where up = Z
    float phi = 2.0 * PI * r1;
    float r = sqrt(r2);
    vec3 local = vec3(r * cos(phi), r * sin(phi), sqrt(1.0 - r2));

    // Two axes lying on the surface, at right angles to the normal and to each other
    vec3 helper = abs(normal.z) < 0.999 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
    vec3 t = normalize(cross(helper, normal));
    vec3 b = cross(normal, t);

    return t * local.x + b * local.y + normal * local.z;
}

// First body hit by the ray: returns its index (-1 = none) and writes its distance into best_t
int closest_hit(vec3 origin, vec3 dir, out float best_t) {
    best_t = 1e30;
    int best_index = -1;
    
    for (int i = 0; i < 16; i++) {
        vec4 body = all_bodies.bodies[i];
        if (body.w == 0.0) { continue; }
        float t = ray_sphere(origin, dir, body.xyz, body.w);
        if (t > 0.0 && t < best_t) {
            best_t = t;
            best_index = i;
        }
    }
    
    return best_index;
}

// Random direction inside a cone (uniform), around `axis`, half-angle given by its cosine
vec3 sample_cone(vec3 axis, float cos_max, float r1, float r2) {
    // Direction around a flat floor's up = Z, at most θ_max away from it
    float phi = 2.0 * PI * r1;
    float cos_t = 1.0 - r2 * (1.0 - cos_max);
    float sin_t = sqrt(1.0 - cos_t * cos_t);
    vec3 local = vec3(sin_t * cos(phi), sin_t * sin(phi), cos_t);

    // Two axes at right angles to the cone's axis and to each other
    vec3 helper = abs(axis.z) < 0.999 ? vec3(0.0, 0.0, 1.0) : vec3(1.0, 0.0, 0.0);
    vec3 t = normalize(cross(helper, axis));
    vec3 b = cross(axis, t);

    return t * local.x + b * local.y + axis * local.z;
}

// Light that the other bodies reflect onto `point` (one bounce): for each body big enough in the sky,
// rays aimed inside its cone (importance sampling), weighted by the cone's solid angle
vec3 bounce_light(int self_index, vec3 point, float r1, float r2) {
    vec3 n = normalize(point - all_bodies.bodies[self_index].xyz);
    vec3 start = point + n * 0.0001;
    vec3 total = vec3(0.0);

    for (int j = 1; j < 16; j++) {
        vec4 body = all_bodies.bodies[j];
        if (body.w == 0.0) { continue; }
        if (j == self_index) { continue; }

        vec3 to_body = body.xyz - start;
        float dist = length(to_body);
        float sin_max = body.w / dist;
        // Too small in the sky (under ~1°) to light us noticeably
        if (sin_max < 0.02) { continue; }

        // The cone the body fills in our sky: edge cosine and solid angle Ω
        float cos_max = sqrt(1.0 - sin_max * sin_max);
        float omega = 2.0 * PI * (1.0 - cos_max);

        // Stratified samples: golden-ratio shifts spread them evenly over the body's disc
        for (int k = 0; k < BOUNCE_SAMPLES; k++) {
            float s1 = fract(r1 + float(k) * 0.618034);
            float s2 = fract(r2 + float(k) * 0.414214);
            vec3 ray = sample_cone(to_body / dist, cos_max, s1, s2);

            float cos_theta = dot(n, ray);
            // Below our horizon: the ray would go into our own ground
            if (cos_theta <= 0.0) { continue; }

            float t;
            int hit_j = closest_hit(start, ray, t);
            // Something else is in the way (another moon, a planet)
            if (hit_j != j) { continue; }

            vec3 b = start + t * ray;
            total += direct_light(j, b) * cos_theta * omega / PI / float(BOUNCE_SAMPLES);
        }
    }

    return total;
}

void main() {
    // The point far away behind this pixel, in the world (screen → world)
    vec4 far = inverse(push.view_proj) * vec4(screen_pos, 1.0, 1.0);
    vec3 far_point = far.xyz / far.w;
    // Ray: from the camera, through this pixel
    vec3 origin = push.camera_pos.xyz;
    vec3 dir = normalize(far_point - origin);

    // Per-pixel random offset in [0, 1) for the atmosphere samples (hash of the pixel position)
    float jitter = fract(sin(dot(gl_FragCoord.xy, vec2(39.3468, 11.1352))) * 43758.5453);
    // Shift it every frame by the golden ratio: successive frames cover [0, 1) evenly
    jitter = fract(jitter + push.camera_pos.w * 0.618034);

    // Second, independent random number (other hash weights, other per-frame shift: √2 − 1)
    float jitter2 = fract(sin(dot(gl_FragCoord.xy, vec2(91.7151, 47.2386))) * 43758.5453);
    jitter2 = fract(jitter2 + push.camera_pos.w * 0.414214);

    // Closest body along the camera ray
    float best_t;
    int best_index = closest_hit(origin, dir, best_t);
    
    // Body 0: the Sun
    if (best_index == 0) {
        // Where the ray hit the Sun, in the world
        vec3 hit = origin + best_t * dir;
        // Its depth on screen: same maths as the planets' vertex shader
        vec4 clip = push.view_proj * vec4(hit, 1.0);
        gl_FragDepth = clip.z / clip.w;

        // Limb darkening: mu = 1 at the disc's centre, 0 at its edge
        vec3 normal = normalize(hit - push.sun.xyz);
        float mu = dot(normal, -dir);
        float brightness = 1.0 - 0.6 * (1.0 - mu);
        
        out_color = vec4(vec3(1.0, 0.97, 0.92) * brightness * SUN_INTENSITY, 1.0); // hit: the Sun
    } else if (best_index > 0) {
        // A planet: where the ray hit it
        vec3 hit = origin + best_t * dir;
        // Its depth on screen
        vec4 clip = push.view_proj * vec4(hit, 1.0);
        gl_FragDepth = clip.z / clip.w;

        vec3 light = direct_light(best_index, hit);

        // Plus light bounced off the other bodies (Earthshine, Jupiter-shine…)
        light += albedo(best_index, hit) * bounce_light(best_index, hit, jitter, jitter2);

        out_color = vec4(light, 1.0);
        
    } else {
        gl_FragDepth = 0.9999;

        float u = atan(dir.y, dir.x) / (2.0 * PI);
        float v = acos(dir.z) / PI;
        vec3 stars = texture(sky, vec2(u, v)).rgb;
        
        // Miss: the sky. Ray direction → equirectangular UV (Z is up)
        out_color = vec4(stars, 1.0);
    }

    // Atmospheres (single scattering): march the view ray through each planet's gas,
    // adding the sunlight each lit sample scatters towards the camera
    for (int i = 1; i <= 15; i++) {
        vec4 atmo = all_bodies.atmospheres[i];
        vec4 haze = all_bodies.haze[i];

        if (atmo.w == 0.0) { continue; }

        vec4 body = all_bodies.bodies[i];
        vec2 shell = ray_sphere_both(origin, dir, body.xyz, body.w * atmo.w);

        if (shell.y < 0.0) { continue; }

        // Part of the ray inside the gas: from the camera at the earliest, up to the first solid hit
        float t_in = max(shell.x, 0.0);
        float t_out = min(shell.y, best_t);

        // Planet radius and scale height (density falls to 37% every H)
        const float R = body.w;
        const float H = (atmo.w - 1.0) * R * 0.2;
        float ds = (t_out - t_in) / VIEW_STEPS;
        // Optical depth from the camera to the current sample (running total)
        float view_depth = 0.0;
        vec3 scatter_r = vec3(0);
        vec3 scatter_m = vec3(0);

        for (int s = 0; s < VIEW_STEPS; s++) {
            // Sample at a random point inside step s (jitter: bands become fine noise)
            float t = t_in + (s + jitter) * ds;
            vec3 p = origin + dir * t;
            float height  = length(p - body.xyz) - R;
            float density = exp(-height / H);
            vec3 sun_dir = normalize(push.sun.xyz - p);

            // Air between the camera and p: counted even in shadow (it still blocks the view)
            view_depth += density * ds / R;
            
            // The planet hides the Sun from this sample: night side, no scattering
            if (ray_sphere(p, sun_dir, body.xyz, R) > 0.0) { continue; }

            // Other bodies eclipsing the Sun for this sample (e.g. Titan in Saturn's shadow)
            float vis = sun_visibility(p, i);

            // Light march: optical depth from p to the top of the atmosphere, towards the Sun
            vec2 light = ray_sphere_both(p, sun_dir, body.xyz, R * atmo.w);
            float l_ds = light.y / LIGHT_STEPS;
            float light_depth = 0.0;
            
            for (int l = 0; l < LIGHT_STEPS; l++) {
                vec3 q = p + sun_dir * (l + jitter) * l_ds;
                float l_height = length(q - body.xyz) - R;
                light_depth += exp(-l_height / H) * l_ds / R;
            }

            // Beer–Lambert through both gases: fraction of each colour surviving Sun → p → camera
            vec3 T = exp(-(atmo.rgb + haze.rgb) * (view_depth + light_depth));

            // Rayleigh phase: twice as much light scattered along the Sun's axis as at 90°
            float mu = dot(dir, sun_dir);
            float phase_r = 3 / (16 * PI) * (1 + mu * mu);
            // Mie phase (Henyey–Greenstein): strongly forward, a bright halo around the Sun
            float g = haze.w;
            float phase_m = (1.0 - g * g) / ( (4.0 * PI) * pow(1.0 + g * g - 2.0 * g * mu, 1.5));

            // Light scattered at p, dimmed on both legs (distances in planet radii, the same unit as β)
            scatter_r += density * ds / R * T * phase_r * vis;
            scatter_m += density * ds / R * T * phase_m * vis;
        }

        // What lies behind the gas (ground, stars, Sun) is seen through all of it
        out_color.rgb *= exp(-(atmo.rgb + haze.rgb) * view_depth);

        // Add the light both gases scattered towards the camera (each with its own β and phase)
        out_color.rgb += SUN_INTENSITY * (atmo.rgb * scatter_r + haze.rgb * scatter_m);
    }
    
    // Dithering: tiny per-pixel noise hides the 256-level steps (banding)
    float noise = fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453);
    // Add the noise in sRGB space (what the screen shows), not in linear space
    vec3 srgb = pow(out_color.rgb, vec3(1.0 / 2.2));
    srgb += (noise - 0.5) / 255.0;
    out_color.rgb = pow(max(srgb, 0.0), vec3(2.2));
}
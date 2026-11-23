#version 450

// Ray-tracing pass: runs once per pixel of the screen

// Position on screen, -1..1 (from raytrace.vert)
layout(location = 0) in vec2 screen_pos;

layout(location = 0) out vec4 out_color;

// Same layout as `RayPushConstants` in renderer.rs
layout(push_constant) uniform Push {
    mat4 view_proj;
    vec4 camera_pos;
    vec4 sun;      // xyz = centre, w = radius
} push;

layout(set = 0, binding = 1) uniform Bodies {
    vec4 bodies[16];
    vec4 colours[16];
    vec4 atmospheres[16]; // rgb = scattering strength per colour (β), w = outer radius ÷ planet radius (0 = none)
} all_bodies;

layout(set = 0, binding = 2) uniform sampler2D sky;
layout(set = 0, binding = 0) uniform sampler2D planet_texture;

// How bright the Sun is compared with a lit planet (HDR: can be much more than 1.0)
const float SUN_INTENSITY = 4.0;
const float PI = 3.14159265;
const int VIEW_STEPS = 32; // samples along the view ray inside an atmosphere
const int LIGHT_STEPS = 8; // samples along each sample's ray towards the Sun

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

void main() {
    // The point far away behind this pixel, in the world (screen → world)
    vec4 far = inverse(push.view_proj) * vec4(screen_pos, 1.0, 1.0);
    vec3 far_point = far.xyz / far.w;
    // Ray: from the camera, through this pixel
    vec3 origin = push.camera_pos.xyz;
    vec3 dir = normalize(far_point - origin);

    // Find the closest body this ray hits (smallest t)
    float best_t = 1e30;
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

        // Diffuse lighting: how much this point faces the Sun
        vec3 normal = normalize(hit - all_bodies.bodies[best_index].xyz);
        float pu = atan(normal.y, normal.x) / (2.0 * PI);
        float pv = acos(normal.z) / PI;
        vec3 light_dir = normalize(push.sun.xyz - hit);
        float brightness = max(dot(normal, light_dir), 0.0);

        // Shadow rays: from this pixel towards the Sun. Does another body block the way?
        float sun_dist = length(push.sun.xyz - hit);
        for (int i = 1; i < 16; i++) {   // from 1: body 0 is the Sun itself
            vec4 other = all_bodies.bodies[i];
    
            if (other.w == 0.0) { continue; }
            if (i == best_index) { continue; }
    
            vec3 to_blocker = other.xyz - hit;
            float blocker_dist = length(to_blocker);
            if (blocker_dist > sun_dist) { continue; }           // beyond the Sun: can't shade us
            
            float a_sun = asin(clamp(all_bodies.bodies[0].w / sun_dist, 0.0, 1.0)); // body 0 = the Sun
            float a_b   = asin(clamp(other.w / blocker_dist, 0.0, 1.0));
            float theta = acos(clamp(dot(normalize(to_blocker), light_dir), -1.0, 1.0));
            
            // How much of the Sun is hidden: 0 = none, up to 1 = all of it
            float max_cover = min(1.0, (a_b * a_b) / (a_sun * a_sun));
            float cover = (1.0 - smoothstep(abs(a_sun - a_b), a_sun + a_b, theta)) * max_cover;
            brightness *= 1.0 - cover;
        }
        
        vec3 base = mix(all_bodies.colours[best_index].rgb, texture(planet_texture, vec2(pu, pv)).rgb, all_bodies.colours[best_index].w);
        out_color = vec4(base * brightness, 1.0);
        
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
    // Per-pixel random offset in [0, 1) for the atmosphere samples (hash, stable from frame to frame)
    float jitter = fract(sin(dot(gl_FragCoord.xy, vec2(39.3468, 11.1352))) * 43758.5453);
    for (int i = 1; i <= 15; i++) {
        vec4 atmo = all_bodies.atmospheres[i];

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
        vec3 scatter = vec3(0);

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

            // Light march: optical depth from p to the top of the atmosphere, towards the Sun
            vec2 light = ray_sphere_both(p, sun_dir, body.xyz, R * atmo.w);
            float l_ds = light.y / LIGHT_STEPS;
            float light_depth = 0.0;
            
            for (int l = 0; l < LIGHT_STEPS; l++) {
                vec3 q = p + sun_dir * (l + jitter) * l_ds;
                float l_height = length(q - body.xyz) - R;
                light_depth += exp(-l_height / H) * l_ds / R;
            }

            // Beer–Lambert: fraction of each colour surviving Sun → p → camera (blue fades first)
            vec3 T = exp(-atmo.rgb * (view_depth + light_depth));

            // Light scattered at p, dimmed on both legs (distances in planet radii, the same unit as β)
            scatter += density * ds / R * T;
        }

        // β gives the colour; 0.1 stands in for the phase function (A2d)
        out_color.rgb += SUN_INTENSITY * atmo.rgb * scatter * 0.1;
    }
    
    // Dithering: tiny per-pixel noise hides the 256-level steps (banding)
    float noise = fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453);
    // Add the noise in sRGB space (what the screen shows), not in linear space
    vec3 srgb = pow(out_color.rgb, vec3(1.0 / 2.2));
    srgb += (noise - 0.5) / 255.0;
    out_color.rgb = pow(max(srgb, 0.0), vec3(2.2));
}
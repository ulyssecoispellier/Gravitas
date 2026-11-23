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
} all_bodies;

// How bright the Sun is compared with a lit planet (HDR: can be much more than 1.0)
const float SUN_INTENSITY = 4.0;

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
        vec3 light_dir = normalize(push.sun.xyz - hit);
        float brightness = max(dot(normal, light_dir), 0.0);

        out_color = vec4(vec3(brightness), 1.0);
    } else {
        gl_FragDepth = 0.9999;

        // Closest distance between this ray and the Sun's centre
        float along = dot(push.sun.xyz - origin, dir);
        vec3 closest = origin + along * dir;
        float d = length(push.sun.xyz - closest);

        // Glow: 0.4 at the Sun's edge (matches the limb), fading fast with distance from the surface
        float glow = exp(-(d - push.sun.w) / (0.15 * push.sun.w)) * 0.4;

        // Sun behind the camera: no glow
        if (along < 0.0) {
            glow = 0.0;
        }

        // Miss: black space + the Sun's glow
        out_color = vec4(vec3(1.0, 0.97, 0.92) * glow * SUN_INTENSITY, 1.0);
    }

    // Dithering: tiny per-pixel noise hides the 256-level steps (banding)
    float noise = fract(sin(dot(gl_FragCoord.xy, vec2(12.9898, 78.233))) * 43758.5453);
    // Add the noise in sRGB space (what the screen shows), not in linear space
    vec3 srgb = pow(out_color.rgb, vec3(1.0 / 2.2));
    srgb += (noise - 0.5) / 255.0;
    out_color.rgb = pow(max(srgb, 0.0), vec3(2.2));
}
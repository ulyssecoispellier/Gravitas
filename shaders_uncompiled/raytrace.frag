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
    
    // Does this pixel's ray hit the Sun?
    float t = ray_sphere(origin, dir, push.sun.xyz, push.sun.w);
    if (t > 0.0) {
        // Where the ray hit the Sun, in the world
        vec3 hit = origin + t * dir;
        // Its depth on screen: same maths as the planets' vertex shader
        vec4 clip = push.view_proj * vec4(hit, 1.0);
        gl_FragDepth = clip.z / clip.w;
        out_color = vec4(1.0, 0.97, 0.92, 1.0); // hit: the Sun
    } else {
        gl_FragDepth = 0.9999;
        out_color = vec4(0.0, 0.0, 0.0, 1.0); // miss: black space
    }
}
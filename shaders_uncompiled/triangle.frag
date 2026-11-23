#version 450

// Fragment shader: runs once per pixel of each triangle, and decides its colour.

// Inputs from the vertex shader, blended between the triangle's 3 vertices
layout(location = 0) in vec3 frag_color;
layout(location = 1) in vec3 frag_normal;
layout(location = 2) in vec3 frag_world_pos;
layout(location = 3) in vec2 frag_uv;

// Output: the pixel's colour (r, g, b, alpha) in the colour attachment
layout(location = 0) out vec4 out_color;

// Same push constants as the vertex shader (see `PushConstants` in renderer.rs)
layout(push_constant) uniform Push {
    mat4 transform;
    vec4 body_pos;
    vec4 light_pos;
    vec4 info;
    vec4 colour;
} push;

// Rocky planet texture (set 0, binding 0)
layout(set = 0, binding = 0) uniform sampler2D planet_texture;
// Every body: xyz = position, w = radius (set 0, binding 1, see `bodies_buffer`)
layout(set = 0, binding = 1) uniform Bodies {
    vec4 bodies[16];
    vec4 colours[16];
} all_bodies;

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
    // Direction from this pixel TOWARDS the star: star position - pixel position, made length 1
    vec3 light_dir = normalize(push.light_pos.xyz - frag_world_pos);
    // Blending between 3 vertices shortens the normal a bit: make it length 1 again
    vec3 normal = normalize(frag_normal);

    // Diffuse light: 1 when facing the light, 0 at 90° or facing away
    float brightness = max(dot(normal, light_dir), 0.0);

    // Shadow rays: from this pixel towards the Sun. Does another body block the way?
    float sun_dist = length(push.light_pos.xyz - frag_world_pos);
    for (int i = 1; i < 16; i++) {   // from 1: body 0 is the Sun itself
        vec4 other = all_bodies.bodies[i];

        if (other.w == 0.0) { continue; }
        if (distance(other.xyz, push.body_pos.xyz) < 0.001) { continue; }

        vec3 to_blocker = other.xyz - frag_world_pos;
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
    
    // A glowing body (the star, w = 1) is fully bright everywhere
    brightness = max(brightness, push.info.y);

    // Base colour: texture if info.z = 1, else the body's colour
    vec3 base = mix(push.colour.rgb, texture(planet_texture, frag_uv).rgb, push.info.z);

    out_color = vec4(base * brightness, 1.0);
}

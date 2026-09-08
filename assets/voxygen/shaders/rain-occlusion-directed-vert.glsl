#version 440 core
// #extension ARB_texture_storage : enable

#include <constants.glsl>

#define LIGHTING_TYPE LIGHTING_TYPE_REFLECTION

#define LIGHTING_REFLECTION_KIND LIGHTING_REFLECTION_KIND_GLOSSY

#if (FLUID_MODE == FLUID_MODE_LOW)
#define LIGHTING_TRANSPORT_MODE LIGHTING_TRANSPORT_MODE_IMPORTANCE
#elif (FLUID_MODE >= FLUID_MODE_MEDIUM)
#define LIGHTING_TRANSPORT_MODE LIGHTING_TRANSPORT_MODE_RADIANCE
#endif

#define LIGHTING_DISTRIBUTION_SCHEME LIGHTING_DISTRIBUTION_SCHEME_MICROFACET

#define LIGHTING_DISTRIBUTION LIGHTING_DISTRIBUTION_BECKMANN

#define HAS_SHADOW_MAPS

// Currently, we only need globals for focus_off.
#include <globals.glsl>
#include <cube.glsl>

layout (std140, set = 0, binding = 14)
uniform u_rain_occlusion {
    mat4 rain_occlusion_matrices;
    mat4 rain_occlusion_texture_mat;
    mat4 rain_dir_mat;
    float integrated_rain_vel;
    float rain_density;
    vec2 occlusion_dummy; // Fix alignment.
};

/* Accurate packed shadow maps for many lights at once!
 *
 * Ideally, we would just write to a bitmask...
 *
 * */

layout(location = 0) in uint v_pos_norm;

// Light projection matrices.
layout (std140, set = 1,  binding = 0)
uniform u_locals {
    mat4 model_mat;
    ivec4 atlas_offs;
    float load_time;
    // Le remplissage que Rust garde ici : il doit apparaitre, sinon ce qui suit
    // tomberait au mauvais endroit.
    float locals_dummy0;
    float locals_dummy1;
    float locals_dummy2;
    // La base 3D de la face qui porte ce chunk, et l'origine de cette face dans
    // le patron (D27). **Elle est deja dans ce tampon** : la passe d'ombre lie
    // les memes `terrain::Locals` que la passe principale, elle n'en declarait
    // simplement qu'une vue tronquee.
    vec4 cube_r;
    vec4 cube_h;
    vec4 cube_n;
    vec4 cube_face;
};

const float EXTRA_NEG_Z = 32768.0;

void main() {
    vec3 f_chunk_pos = vec3(v_pos_norm & 0x3Fu, (v_pos_norm >> 6) & 0x3Fu, float((v_pos_norm >> 12) & 0xFFFFu) - EXTRA_NEG_Z);
    vec3 f_pos = (model_mat * vec4(f_chunk_pos, 1.0)).xyz - focus_off.xyz;

    // La courbure du monde (D27), exactement comme la passe principale. Sans
    // elle, la geometrie qui projette les ombres n'est pas celle qu'on voit :
    // les ombres tombent a cote, et d'autant plus loin qu'on regarde loin.
    if (cube.z > 0.5) {
        vec3 absolu = f_pos + focus_off.xyz;
        f_pos = cube_projeter(
            absolu.xy - cube_face.xy,
            absolu.z,
            cube_r.xyz,
            cube_h.xyz,
            cube_n.xyz
        );
    }
    
    gl_Position = rain_occlusion_matrices * vec4(f_pos, 1.0);
}

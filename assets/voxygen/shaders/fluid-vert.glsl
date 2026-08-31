#version 440 core

#include <constants.glsl>

#define LIGHTING_TYPE (LIGHTING_TYPE_TRANSMISSION | LIGHTING_TYPE_REFLECTION)

#define LIGHTING_REFLECTION_KIND LIGHTING_REFLECTION_KIND_SPECULAR

#if (FLUID_MODE == FLUID_MODE_LOW)
#define LIGHTING_TRANSPORT_MODE LIGHTING_TRANSPORT_MODE_IMPORTANCE
#elif (FLUID_MODE >= FLUID_MODE_MEDIUM)
#define LIGHTING_TRANSPORT_MODE LIGHTING_TRANSPORT_MODE_RADIANCE
#endif

#define LIGHTING_DISTRIBUTION_SCHEME LIGHTING_DISTRIBUTION_SCHEME_MICROFACET

#define LIGHTING_DISTRIBUTION LIGHTING_DISTRIBUTION_BECKMANN

#include <globals.glsl>
#include <srgb.glsl>
#include <random.glsl>
#include <cube.glsl>

layout(location = 0) in uint v_pos_norm;
layout(location = 1) in uint v_vel;

layout(std140, set = 2, binding = 0)
uniform u_locals {
    mat4 model_mat;
    ivec4 atlas_offs;
    float load_time;
    // Le remplissage que Rust garde ici, puis la base de face (D27). L'eau
    // reçoit les `Locals` du terrain : les champs sont deja dans le tampon.
    float locals_dummy0;
    float locals_dummy1;
    float locals_dummy2;
    vec4 cube_r;
    vec4 cube_h;
    vec4 cube_n;
    vec4 cube_face;
};

layout(location = 0) out vec3 f_pos;
layout(location = 1) flat out uint f_pos_norm;
layout(location = 2) out vec2 f_vel;

const float EXTRA_NEG_Z = 65536.0;

void main() {
    vec3 rel_pos = vec3(v_pos_norm & 0x3Fu, (v_pos_norm >> 6) & 0x3Fu, float((v_pos_norm >> 12) & 0x1FFFFu) - EXTRA_NEG_Z);
    f_pos = (model_mat * vec4(rel_pos, 1.0)).xyz - focus_off.xyz;

    f_vel = vec2(
        (float(v_vel & 0xFFFFu) - 32768.0) / 1000.0,
        (float((v_vel >> 16u) & 0xFFFFu) - 32768.0) / 1000.0
    );

    // Terrain 'pop-in' effect
    #ifndef EXPERIMENTAL_BAREMINIMUM
        #ifdef EXPERIMENTAL_TERRAINPOP
            f_pos.z -= 250.0 * (1.0 - min(1.0001 - 0.02 / pow(time_since(load_time), 10.0), 1.0));
        #endif
    #endif

    // La courbure du monde (D27). L'eau déforme `f_pos` lui-même, faute d'une
    // position séparée pour l'affichage : c'est ce que le crochet d'origine
    // faisait déjà.
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

    #ifdef EXPERIMENTAL_CURVEDWORLD
        f_pos.z -= pow(distance(f_pos.xy + focus_off.xy, focus_pos.xy + focus_off.xy) * 0.05, 2);
    #endif

    f_pos_norm = v_pos_norm;

    gl_Position = all_mat * vec4(f_pos, 1);
}

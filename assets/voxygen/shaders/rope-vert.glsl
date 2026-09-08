#version 440 core

#include <constants.glsl>

#define FIGURE_SHADER

#define LIGHTING_TYPE LIGHTING_TYPE_REFLECTION

#define LIGHTING_REFLECTION_KIND LIGHTING_REFLECTION_KIND_GLOSSY

#define LIGHTING_TRANSPORT_MODE LIGHTING_TRANSPORT_MODE_IMPORTANCE

#define LIGHTING_DISTRIBUTION_SCHEME LIGHTING_DISTRIBUTION_SCHEME_MICROFACET

#define LIGHTING_DISTRIBUTION LIGHTING_DISTRIBUTION_BECKMANN

#include <globals.glsl>
#include <sky.glsl>

layout(location = 0) in vec3 v_pos;
layout(location = 1) in vec3 v_norm;

layout (std140, set = 2, binding = 0)
uniform u_locals {
    vec4 pos_a;
    vec4 pos_b;
    float rope_length;
};

layout(location = 0) out vec3 f_pos;
layout(location = 1) out vec3 f_norm;
layout(location = 2) out vec3 m_pos;

void main() {
    m_pos = v_pos;

    // Une longe est un cylindre bâti entre deux bouts, et tout y supposait un
    // monde plat : le repère se prenait sur `vec3(0, 0, 1)` et le ventre
    // pendait le long de `−Z`. Sur la face `+X`, ça le fait pendre de côté.
    vec3 rx, ry, rz, pos;
    float dist;
    float dip;

    if (cube_actif()) {
        // Les deux bouts arrivent déjà décalés du foyer : on leur rend le
        // décalage pour les projeter, et le repère se construit sur la
        // verticale **du lieu**.
        vec3 place_a = cube_poser(pos_a.xyz + focus_off.xyz, pos_a.xyz);
        vec3 place_b = cube_poser(pos_b.xyz + focus_off.xyz, pos_b.xyz);
        vec3 haut = cube_direction_de_rendu(place_a);

        rz = normalize(place_b - place_a);
        rx = normalize(cross(haut, rz));
        ry = normalize(cross(rz, rx));
        dist = distance(place_a, place_b);
        pos = place_a + (rx * v_pos.x + ry * v_pos.y) * 0.1 + rz * v_pos.z * dist;

        vec2 ideal_wind_sway = wind_vel * vec2(
            wind_wave(pos.y * 1.5, 1.9, wind_vel.x, wind_vel.y),
            wind_wave(pos.x * 1.5, 2.1, wind_vel.y, wind_vel.x)
        );
        dip = (1 - pow(abs(v_pos.z - 0.5) * 2.0, 2)) * max(rope_length - dist, 0.0);

        // Le balancement dans le plan tangent, le ventre le long de la
        // verticale du lieu.
        vec3 est_l, nord_l;
        cube_exp(cube_log(haut), est_l, nord_l);
        vec2 sway = ideal_wind_sway * min(pow(dip, 2), 0.005);
        pos += est_l * sway.x + nord_l * sway.y - haut * (0.5 * dip);

        f_pos = pos;
    } else {
    rz = normalize(pos_b.xyz - pos_a.xyz);
    rx = normalize(cross(vec3(0, 0, 1), rz));
    ry = normalize(cross(rz, rx));
    dist = distance(pos_a.xyz, pos_b.xyz);
    pos = pos_a.xyz + (rx * v_pos.x + ry * v_pos.y) * 0.1 + rz * v_pos.z * dist;
    vec2 ideal_wind_sway = wind_vel * vec2(
        wind_wave(pos.y * 1.5, 1.9, wind_vel.x, wind_vel.y),
        wind_wave(pos.x * 1.5, 2.1, wind_vel.y, wind_vel.x)
    );
    dip = (1 - pow(abs(v_pos.z - 0.5) * 2.0, 2)) * max(rope_length - dist, 0.0);
    pos += vec3(ideal_wind_sway * min(pow(dip, 2), 0.005), -0.5 * dip);

    f_pos = pos + focus_pos.xyz;

    #ifdef EXPERIMENTAL_CURVEDWORLD
        f_pos.z -= pow(distance(f_pos.xy + focus_off.xy, focus_pos.xy + focus_off.xy) * 0.05, 2);
    #endif
    }

    f_norm = rx * v_norm.x + ry * v_norm.y + rz * v_norm.z;

    gl_Position = all_mat * vec4(f_pos, 1);
}

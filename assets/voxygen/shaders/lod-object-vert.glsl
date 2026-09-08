#version 440 core

#include <constants.glsl>

#define LIGHTING_TYPE LIGHTING_TYPE_REFLECTION

#define LIGHTING_REFLECTION_KIND LIGHTING_REFLECTION_KIND_GLOSSY

#define LIGHTING_TRANSPORT_MODE LIGHTING_TRANSPORT_MODE_IMPORTANCE

#define LIGHTING_DISTRIBUTION_SCHEME LIGHTING_DISTRIBUTION_SCHEME_MICROFACET

#define LIGHTING_DISTRIBUTION LIGHTING_DISTRIBUTION_BECKMANN

#include <globals.glsl>
#include <srgb.glsl>
#include <random.glsl>
#include <lod.glsl>
#include <cube.glsl>

layout(location = 0) in vec3 v_pos;
layout(location = 1) in vec3 v_norm;
layout(location = 2) in vec3 v_col;
layout(location = 3) in uint v_flags;
layout(location = 4) in vec3 inst_pos;
layout(location = 5) in vec3 inst_col;
layout(location = 6) in uint inst_flags;

const uint FLAG_INST_COLOR = 1;
const uint FLAG_INST_GLOW = 2;

const uint FLAG_INST_ROTATION = 4 | 8;

layout(location = 0) out vec3 f_pos;
layout(location = 1) out vec3 f_norm;
layout(location = 2) out vec4 f_col;
layout(location = 3) out vec3 model_pos;
layout(location = 4) flat out uint f_flags;

void main() {
    vec3 obj_pos = inst_pos - focus_off.xyz;
    uint rot_bits = (inst_flags & FLAG_INST_ROTATION) >> 2;

    float sign = float(rot_bits >> 1) * 2.0 - 1.0;
    float d_y = (rot_bits & 1) != 0u ? 1.0 : 0.0;
    float d_x = 1.0 - d_y;
    mat2 rot = sign * mat2(d_x, -d_y, d_y, d_x);

    vec3 local_pos = vec3(rot * v_pos.xy, v_pos.z);
    f_pos = obj_pos + local_pos;
    model_pos = v_pos;

    if (cube_actif()) {
        // Un arbre lointain est petit devant le rayon : la projection ne lui
        // fait subir qu'une transformation rigide, et le repère se prend à son
        // ancre — jamais par sommet, ce qui le déchirerait à une couture.
        // C'est le même chemin que les particules.
        f_pos = cube_poser(inst_pos, f_pos);
        vec3 ancre = cube_poser(inst_pos, obj_pos);
        vec3 haut = normalize(ancre + cube_origine.xyz);

        // La distance **géodésique** au foyer, qui est ce que la version plate
        // appelle sa distance horizontale : l'angle entre les deux verticales,
        // multiplié par le rayon. Une différence de coordonnées du patron ne
        // dirait rien — au-delà d'une couture, deux voisins sont séparés d'une
        // face entière — et une corde 3D compterait en plus le dénivelé, que la
        // version plate ignore.
        float portee = cube.x * acos(clamp(dot(haut, cube_haut()), -1.0, 1.0));
        float portee2 = portee * portee;

        // **Le retrait est radial, et il s'enfonce peu.** Enfoncer de dix mille
        // sur l'axe Z du monde ne ferait pas descendre un objet de la face +X :
        // ça le translaterait de côté. Et radialement, dix mille blocs sur un
        // rayon de cinq mille ressortent **de l'autre côté de la planète**.
        // Sur une sphère, enfoncer ne supprime pas, ça creuse : on s'en tient
        // donc à la même profondeur modeste que la nappe.
        float enfoncement = CUBE_ENFONCEMENT;
        #ifdef EXPERIMENTAL_TERRAINPOP
            float pull_down = min(1.0 / pow(portee / (view_distance.x * 0.95), 150.0), enfoncement);
            f_pos -= haut * pull_down;
        #else
            f_pos -= haut * step(portee2, pow(view_distance.x * 0.95, 2)) * enfoncement;
        #endif
    } else {
    #ifdef EXPERIMENTAL_TERRAINPOP
        float pull_down = 1.0 / pow(distance(focus_pos.xy, obj_pos.xy) / (view_distance.x * 0.95), 150.0);
        f_pos.z -= pull_down;
    #else
        f_pos.z -= step(dot(focus_pos.xy - obj_pos.xy, focus_pos.xy - obj_pos.xy), pow(view_distance.x * 0.95, 2)) * 10000.0;
    #endif

    #ifdef EXPERIMENTAL_CURVEDWORLD
        f_pos.z -= pow(distance(f_pos.xy + focus_off.xy, focus_pos.xy + focus_off.xy) * 0.05, 2);
    #endif
    }

    f_norm = vec3(rot * v_norm.xy, v_norm.z);

    f_col = vec4((v_flags & FLAG_INST_COLOR) != 0u ? inst_col : v_col, 1.0);
    f_flags = inst_flags | (v_flags & FLAG_INST_GLOW);

    gl_Position = all_mat * vec4(f_pos, 1);
}

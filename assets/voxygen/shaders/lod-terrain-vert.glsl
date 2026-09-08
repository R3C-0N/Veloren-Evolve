#version 440 core

#include <constants.glsl>

#define LIGHTING_TYPE LIGHTING_TYPE_REFLECTION

#define LIGHTING_REFLECTION_KIND LIGHTING_REFLECTION_KIND_GLOSSY

#if (FLUID_MODE == FLUID_MODE_LOW)
#define LIGHTING_TRANSPORT_MODE LIGHTING_TRANSPORT_MODE_IMPORTANCE
#elif (FLUID_MODE >= FLUID_MODE_MEDIUM)
#define LIGHTING_TRANSPORT_MODE LIGHTING_TRANSPORT_MODE_RADIANCE
#endif

#define LIGHTING_DISTRIBUTION_SCHEME LIGHTING_DISTRIBUTION_SCHEME_VOXEL

#define LIGHTING_DISTRIBUTION LIGHTING_DISTRIBUTION_BECKMANN

#include <globals.glsl>
#include <srgb.glsl>
#include <lod.glsl>

layout(location = 0) in vec2 v_pos;

layout(location = 0) out vec3 f_pos;
layout(location = 1) out vec3 f_norm;
layout(location = 2) out float pull_down;

void main() {
    if (cube_actif()) {
        // Le disque géodésique (D27). La grille `[-1, 1]²` n'est plus qu'un
        // domaine de paramètres : ce qu'elle paramètre est une calotte centrée
        // sur le joueur, close à l'horizon. Aucun rectangle n'est balayé, donc
        // aucun emplacement mort n'est atteint.
        float portee;
        vec3 dir = cube_lod_pos(v_pos, f_norm, portee);

        // `portee` **est** la distance géodésique, par construction de la
        // carte exponentielle : jamais une différence de coordonnées.
        //
        // **Et le retrait s'enfonce peu.**
        //
        // Cette expression explose en deçà de la distance de vue — à cinquante
        // blocs du joueur elle vaut 4,8·10¹⁵. Sur une carte plate, ce nombre
        // pousse la géométrie hors du tronc de vue et l'y fait **supprimer**.
        // Sur une sphère, enfoncer ne supprime pas : ça **creuse**. Un premier
        // essai borné à `R/2` laissait une fosse cylindrique de 1 400 blocs de
        // rayon et 2 518 de profondeur, à fond plat, centrée sur le joueur —
        // finie, dessinée, dans le champ. Le moindre trou du terrain chargé
        // l'exposait, et sa paroi était le mur qu'on voyait à la limite de vue.
        //
        // Trois cents blocs, donc : plus profond que le relief à l'intérieur
        // d'un chunk, donc la nappe reste sous le terrain réel ; assez peu pour
        // qu'un trou de chargement montre un sol plausible au lieu d'un
        // gouffre, et pour qu'à l'échelle de la planète la fossette ne se voie
        // pas.
        pull_down = min(1.0 / pow(portee / (view_distance.x * 0.95), 20.0), CUBE_ENFONCEMENT);

        // **Les retraits sont radiaux.** Sur une planète, la verticale d'un
        // point de la face +X est +X : retirer sur l'axe Z du monde ne le ferait
        // pas descendre, ça le translaterait de côté — et il reparaîtrait dans
        // le ciel. Le `0.1` est le même que sur une carte plate, contre le
        // combat de profondeur au ras de l'océan.
        f_pos = dir * (cube.x + cube_alt_at(dir) - pull_down - 0.1) - cube_origine.xyz;
    } else {
    // Find distances between vertices. Pull down a tiny bit more to reduce z fighting near the ocean.
    f_pos = lod_pos(v_pos, focus_pos.xy) - vec3(0, 0, 0.1);
    #ifndef EXPERIMENTAL_BAREMINIMUM
        vec2 dims = vec2(1.0 / view_distance.y);
        vec4 f_square = focus_pos.xyxy + vec4(splay(v_pos - dims), splay(v_pos + dims));
        f_norm = lod_norm(f_pos.xy, f_square);
    #endif
    
    pull_down = 1.0 / pow(distance(focus_pos.xy, f_pos.xy) / (view_distance.x * 0.95), 20.0);
    f_pos.z -= pull_down;

    #ifdef EXPERIMENTAL_CURVEDWORLD
        f_pos.z -= pow(distance(f_pos.xy + focus_off.xy, focus_pos.xy + focus_off.xy) * 0.05, 2);
    #endif
    }

    gl_Position = all_mat * vec4(f_pos, 1);
}

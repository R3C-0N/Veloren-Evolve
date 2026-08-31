#ifndef CUBE_GLSL
#define CUBE_GLSL

// La projection du cube sur la sphère, côté shader (D27).
//
// **Ce fichier refait ce que fait le CPU, pas quelque chose d'équivalent.** La
// table conforme est la seule définition de la forme du monde ; si le rendu et
// la logique en avaient deux, la visée tomberait à côté du bloc surligné. La
// bilinéaire ci-dessous est donc écrite à la main, texel par texel, exactement
// comme `Table::ab` — un échantillonneur matériel n'offre aucune garantie
// d'arrondi, et c'est pourquoi la texture est déclarée non filtrable.

#include <globals.glsl>

// La table, en Rg32Float : `(a, b)` gnomonique pour chaque `(s, t)`.
layout(set = 0, binding = 15) uniform texture2D t_conforme;

// Côté de la table. Doit valoir `conforme::N`.
const int CUBE_TABLE_N = 513;

// `(a, b)` interpolé, pour `(s, t)` dans [-1, 1]².
vec2 cube_table_ab(vec2 st) {
    float n = float(CUBE_TABLE_N - 1);
    // Le CPU borne à `n - 1e-9` ; en f32 la différence est sous la résolution.
    vec2 p = clamp((st + 1.0) * 0.5 * n, vec2(0.0), vec2(n - 0.0001));
    ivec2 i = ivec2(p);
    vec2 f = p - vec2(i);

    vec2 a = texelFetch(t_conforme, i + ivec2(0, 0), 0).xy;
    vec2 b = texelFetch(t_conforme, i + ivec2(1, 0), 0).xy;
    vec2 c = texelFetch(t_conforme, i + ivec2(0, 1), 0).xy;
    vec2 d = texelFetch(t_conforme, i + ivec2(1, 1), 0).xy;

    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// La direction du monde pour une position **locale à une face**, en blocs.
//
// La base de la face arrive autrement — par chunk pour le terrain, par instance
// pour le reste : le shader n'a donc jamais à replier quoi que ce soit, ce qui
// lui épargne la boucle de bascule des bases.
vec3 cube_direction(vec2 uv, vec3 base_r, vec3 base_h, vec3 base_n) {
    vec2 st = 2.0 * uv / cube.y - 1.0;
    vec2 ab = cube_table_ab(st);
    return normalize(base_n + base_r * ab.x + base_h * ab.y);
}

// La position projetée d'un point, ramenée au point de convergence.
//
// Le retrait de `cube_origine` joue le rôle que `focus_off` joue sur une carte
// plate : sans lui, les sommets vivraient à quelques milliers de blocs de
// l'origine et la précision des `f32` y laisserait des plumes.
vec3 cube_projeter(vec2 uv, float altitude, vec3 base_r, vec3 base_h, vec3 base_n) {
    vec3 dir = cube_direction(uv, base_r, base_h, base_n);
    return dir * (cube.x + altitude) - cube_origine.xyz;
}

// Le monde est-il un patron de cube ?
bool cube_actif() { return cube.z > 0.5; }

// Les six bases de face, et la place de chacune dans le patron (D27).
//
// Elles ne servent qu'aux objets **sans chunk** — une particule n'appartient à
// aucun. Tout ce qui en a un reçoit sa base toute faite, ce qui vaut mieux :
// aucune recherche, et tous les sommets d'un même objet voient la même face.
const vec3 CUBE_BASES_R[6] = vec3[6](
    vec3( 0.0,  1.0,  0.0), vec3(-1.0,  0.0,  0.0), vec3( 0.0, -1.0,  0.0),
    vec3( 1.0,  0.0,  0.0), vec3(-1.0,  0.0,  0.0), vec3(-1.0,  0.0,  0.0)
);
const vec3 CUBE_BASES_H[6] = vec3[6](
    vec3( 0.0,  0.0,  1.0), vec3( 0.0,  0.0,  1.0), vec3( 0.0,  0.0,  1.0),
    vec3( 0.0,  0.0,  1.0), vec3( 0.0, -1.0,  0.0), vec3( 0.0,  1.0,  0.0)
);
const vec3 CUBE_BASES_N[6] = vec3[6](
    vec3( 1.0,  0.0,  0.0), vec3( 0.0,  1.0,  0.0), vec3(-1.0,  0.0,  0.0),
    vec3( 0.0, -1.0,  0.0), vec3( 0.0,  0.0,  1.0), vec3( 0.0,  0.0, -1.0)
);

// La face qui contient une position du monde. `-1` sur un emplacement mort.
int cube_face_de(vec2 wpos, out vec2 origine) {
    ivec2 case_ = ivec2(floor(wpos / cube.y));
    origine = vec2(case_) * cube.y;
    if (case_.y == 1 && case_.x >= 0 && case_.x <= 3) { return case_.x; }
    if (case_.x == 1 && case_.y == 2) { return 4; }
    if (case_.x == 1 && case_.y == 0) { return 5; }
    return -1;
}

// **Pose un objet plat sur la planète, rigidement.**
//
// Pour ce qui n'a pas de chunk où ranger sa face : on prend le repère à l'ancre
// de l'objet — jamais par sommet, ce qui le déchirerait à une couture — et on y
// applique son déplacement local. Un objet petit devant le rayon ne subit de la
// projection qu'une transformation rigide (D29).
//
// `ancre` est la position du monde de l'objet ; `plat` sa position rendue telle
// que la carte plate l'aurait donnée, c'est-à-dire déjà diminuée de `focus_off`.
vec3 cube_poser(vec3 ancre, vec3 plat) {
    vec2 origine_face;
    int face = cube_face_de(ancre.xy, origine_face);
    if (face < 0) { return plat; }

    vec3 r = CUBE_BASES_R[face];
    vec3 h = CUBE_BASES_H[face];
    vec3 n = CUBE_BASES_N[face];
    vec2 uv = ancre.xy - origine_face;

    vec3 haut = cube_direction(uv, r, h, n);
    vec3 tv = cube_direction(uv + vec2(0.0, 0.5), r, h, n)
            - cube_direction(uv - vec2(0.0, 0.5), r, h, n);
    vec3 nord = normalize(tv - haut * dot(tv, haut));
    vec3 est = cross(nord, haut);

    vec3 place = haut * (cube.x + ancre.z) - cube_origine.xyz;
    vec3 d = plat - (ancre - focus_off.xyz);
    return place + est * d.x + nord * d.y + haut * d.z;
}

#endif

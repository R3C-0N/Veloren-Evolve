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

// Les deux tables se lisent au texel pres, sans echantillonneur : il faut donc
// les fonctions samplerless. `srgb.glsl` les active pour les shaders qui
// l'incluent — la passe d'ombre, elle, ne l'inclut pas.
#extension GL_EXT_samplerless_texture_functions : enable

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

// --------------------------------------------------------------------------
// L'inverse : d'une direction du monde vers une place dans le patron
// --------------------------------------------------------------------------

// La table inverse, en Rg32Float : `(s, t)` de face pour chaque `(a, b)`
// gnomonique. Même discipline que la directe — non filtrable, bilinéaire
// écrite à la main, et c'est le CPU qui en donne les octets.
// **17, pas 16 :** `sprite-vert.glsl` occupe déjà 16 pour son tampon de
// sommets, qui s'ajoute à la suite de la disposition de base.
layout(set = 0, binding = 17) uniform texture2D t_conforme_inverse;

// Côté de la table inverse. Doit valoir `conforme::COTE_INVERSE`.
const int CUBE_INVERSE_N = 513;

// La place d'une face dans le patron, en cases. Doit valoir `cube::PATRON`.
const vec2 CUBE_PATRON[6] = vec2[6](
    vec2(0.0, 1.0), vec2(1.0, 1.0), vec2(2.0, 1.0),
    vec2(3.0, 1.0), vec2(1.0, 2.0), vec2(1.0, 0.0)
);

// `(s, t)` interpolé, pour `(a, b)` dans [-1, 1]².
vec2 cube_inverse_st(vec2 ab) {
    float n = float(CUBE_INVERSE_N - 1);
    vec2 p = clamp((ab + 1.0) * 0.5 * n, vec2(0.0), vec2(n - 0.0001));
    ivec2 i = ivec2(p);
    vec2 f = p - vec2(i);

    vec2 a = texelFetch(t_conforme_inverse, i + ivec2(0, 0), 0).xy;
    vec2 b = texelFetch(t_conforme_inverse, i + ivec2(1, 0), 0).xy;
    vec2 c = texelFetch(t_conforme_inverse, i + ivec2(0, 1), 0).xy;
    vec2 d = texelFetch(t_conforme_inverse, i + ivec2(1, 1), 0).xy;

    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// La face d'où vient une direction, et sa place `(s, t)` dedans.
//
// Aucune étape n'a de pôle : la face est celle dont la normale domine, donc
// toujours l'une des six, et jamais un emplacement mort. C'est structurel, pas
// une borne posée après coup — et c'est la différence avec le balayage de
// rectangle que la nappe faisait.
int cube_face_de_direction(vec3 d, out vec2 st) {
    int face = 0;
    float meilleur = -2.0;
    for (int f = 0; f < 6; f++) {
        float p = dot(d, CUBE_BASES_N[f]);
        if (p > meilleur) { meilleur = p; face = f; }
    }
    // `meilleur` vaut au moins 1/racine(3) : la division gnomonique ne peut
    // pas exploser.
    st = cube_inverse_st(vec2(
        dot(d, CUBE_BASES_R[face]) / meilleur,
        dot(d, CUBE_BASES_H[face]) / meilleur
    ));
    return face;
}

// La position du patron d'où vient une direction. L'inverse de
// `cube_direction`, table comprise.
vec2 cube_wpos_de_direction(vec3 d) {
    vec2 st;
    int face = cube_face_de_direction(d, st);
    return (st + 1.0) * 0.5 * cube.y + CUBE_PATRON[face] * cube.y;
}

// La direction d'où vient une position **de rendu**.
//
// Gratuite, et c'est ce qui évite un varying : tout point rendu vaut
// `dir*(R + alt) - cube_origine`, avec `R + alt > 0`. Un varying, lui, serait
// interpolé à travers une couture et mentirait exactement là où il ne faut pas.
vec3 cube_direction_de_rendu(vec3 f_pos) {
    return normalize(f_pos + cube_origine.xyz);
}

// --------------------------------------------------------------------------
// Le repère du foyer, et la carte exponentielle
// --------------------------------------------------------------------------

// **De combien on enfonce ce qu'on veut cacher sous le terrain chargé.**
//
// Sur une carte plate, enfoncer c'est supprimer : la géométrie sort du tronc de
// vue. Sur une sphère, enfoncer c'est **creuser** — la fosse reste finie et
// dessinée. Cette profondeur est donc un compromis : plus profonde que le relief
// d'un chunk, pour que la nappe reste sous le terrain ; assez faible pour qu'un
// trou de chargement montre un sol plausible, et pour ne rien creuser de visible
// à l'échelle de la planète.
const float CUBE_ENFONCEMENT = 300.0;

// La verticale au foyer. Elle ne voyage pas : `cube_origine` est déjà
// `dir(foyer) * R`.
vec3 cube_haut() { return normalize(cube_origine.xyz); }

vec3 cube_est() {
    vec3 haut = cube_haut();
    return normalize(cube_repere.xyz - haut * dot(cube_repere.xyz, haut));
}

vec3 cube_nord() { return cross(cube_haut(), cube_est()); }

// Portée angulaire du lointain : au-delà, il n'y a plus rien à dessiner.
float cube_portee() { return cube_repere.w; }

// **La carte exponentielle : d'un déplacement dans le plan tangent au foyer
// vers un point de la sphère.**
//
// C'est ce qui remplace le balayage de rectangle. `d` est un déplacement en
// blocs, mesuré depuis le joueur ; `length(d)` est exactement la distance
// géodésique parcourue, par construction — il n'y a donc jamais de différence
// de coordonnées à prendre pour une distance.
//
// Le repère au point atteint sort **analytiquement**, pas par différences
// finies : le long du méridien la dérivée est connue, et la perpendiculaire est
// transportée telle quelle. Quatre lectures de table économisées par sommet.
vec3 cube_exp(vec2 d, out vec3 est_p, out vec3 nord_p) {
    vec3 haut = cube_haut();
    // Le CPU envoie l'est du même lieu que `cube_origine`, donc déjà
    // perpendiculaire ; on le réorthogonalise quand même, parce que rien dans le
    // type ne le garantit et qu'un repère qui dérive tord la nappe entière.
    vec3 est = normalize(cube_repere.xyz - haut * dot(cube_repere.xyz, haut));
    vec3 nord = cross(haut, est);

    float r = length(d);
    if (r < 1e-6) {
        est_p = est;
        nord_p = nord;
        return haut;
    }

    vec2 u = d / r;                       // (cos phi, sin phi)
    vec3 m = est * u.x + nord * u.y;      // la direction du méridien suivi
    float theta = r / cube.x;
    float c = cos(theta);
    float s = sin(theta);

    vec3 dir = haut * c + m * s;
    vec3 dm = -haut * s + m * c;          // la dérivée selon theta, unitaire
    vec3 perp = -est * u.y + nord * u.x;  // la perpendiculaire, transportée
    est_p = dm * u.x - perp * u.y;
    nord_p = dm * u.y + perp * u.x;
    return dir;
}

// **L'inverse de la carte exponentielle.**
//
// D'une direction du monde vers le déplacement du plan tangent qui y mène. Deux
// fonctions trigonométriques, et c'est ce qui donne un **champ de repères
// continu** sur toute la calotte : un repère choisi au hasard perpendiculairement
// à `dir` tournerait d'un fragment à l'autre et ferait scintiller tout ce qui
// s'y appuie. Ici, deux points voisins ont des coordonnées voisines.
//
// C'est la carte des coordonnées normales géodésiques : au centre elle est une
// isométrie, et elle ne se déforme qu'en s'éloignant — d'un facteur `sin θ / θ`
// dans la direction azimutale, soit 16 % à un radian.
vec2 cube_log(vec3 dir) {
    vec3 haut = cube_haut();
    float c = clamp(dot(haut, dir), -1.0, 1.0);
    vec3 m = dir - haut * c;
    float l = length(m);
    if (l < 1e-7) { return vec2(0.0); }
    m /= l;
    float theta = acos(c);
    return vec2(dot(m, cube_est()), dot(m, cube_nord())) * (theta * cube.x);
}

// La hauteur d'une position **de rendu** au-dessus de la sphère de référence.
//
// C'est ce que « altitude » veut dire sur une planète. Le `.z` d'une position de
// rendu, lui, ne veut rien dire : il vit dans le repère 3D du cube.
float cube_hauteur_rendu(vec3 f_pos) {
    return length(f_pos + cube_origine.xyz) - cube.x;
}

// Le **nord** local en un point de rendu.
//
// À ne pas confondre avec la verticale, et le piège n'est pas théorique : dans
// `get_sun_diffuse2`, deux lignes voisines demandent l'une le nord — la pente
// de la surface, `sin β = north ⋅ norm` — et l'autre la verticale. Les avoir
// confondues divisait par dix la lumière ambiante de tout le terrain.
//
// Sur une carte plate, le nord vaut exactement `+Y` : c'est ce qui permet à la
// substitution de ne rien changer là-bas.
vec3 cube_nord_en(vec3 f_pos) {
    vec3 e, n;
    cube_exp(cube_log(cube_direction_de_rendu(f_pos)), e, n);
    return n;
}

// L'élévation d'une direction du monde au-dessus de l'horizon **du lieu**.
//
// Le soleil garde une direction unique pour toute la planète — il est loin, et
// c'est la bonne physique. Ce qui est local, c'est sa hauteur au-dessus de
// l'horizon : d'où le jour et la nuit en même temps sur le globe.
float cube_elevation(vec3 d, vec3 haut) { return dot(d, haut); }

// --------------------------------------------------------------------------
// Lire l'atlas sans déborder d'une face
// --------------------------------------------------------------------------

// Combien de chunks fait l'arête d'une face, dans cet atlas. Le patron fait
// quatre faces de large, toujours.
float cube_atlas_face(texture2D tex) {
    return float(textureSize(tex, 0).x) * 0.25;
}

// Un texel de l'atlas, **borné au rectangle de la face**.
//
// Deux faces voisines dans le patron ne le sont pas dans le monde : laisser le
// filtre déborder ferait baver l'une sur l'autre exactement aux coutures, là où
// tout se joue. Au pire on répète le texel du bord sur un demi-texel, ce qui ne
// se voit pas ; mélanger deux faces, si. C'est la leçon que le globe de D38 a
// déjà payée.
ivec2 cube_atlas_texel(int face, float fc, vec2 p0, vec2 dp) {
    return ivec2(CUBE_PATRON[face] * fc + clamp(p0 + dp, vec2(0.0), vec2(fc - 1.0)));
}

// L'altitude brute, deux octets par texel, **décodée avant mélange**.
//
// La borne oblige à écrire la bilinéaire à la main, et c'est une bonne
// nouvelle : `alt_at` décode ses deux octets *après* filtrage matériel, si bien
// qu'un enroulement de l'octet de poids faible entre deux texels y fabrique un
// pic. Décoder par texel puis mélanger n'a pas ce défaut.
float cube_atlas_alt16(texture2D tex, int face, vec2 st) {
    float fc = cube_atlas_face(tex);
    vec2 p = (st + 1.0) * 0.5 * fc - 0.5;
    vec2 p0 = floor(p);
    vec2 fr = p - p0;

    vec4 ta = texelFetch(tex, cube_atlas_texel(face, fc, p0, vec2(0.0, 0.0)), 0);
    vec4 tb = texelFetch(tex, cube_atlas_texel(face, fc, p0, vec2(1.0, 0.0)), 0);
    vec4 tc = texelFetch(tex, cube_atlas_texel(face, fc, p0, vec2(0.0, 1.0)), 0);
    vec4 td = texelFetch(tex, cube_atlas_texel(face, fc, p0, vec2(1.0, 1.0)), 0);

    float a = ta.r * (1.0 / 256.0) + ta.g;
    float b = tb.r * (1.0 / 256.0) + tb.g;
    float c = tc.r * (1.0 / 256.0) + tc.g;
    float d = td.r * (1.0 / 256.0) + td.g;

    return mix(mix(a, b, fr.x), mix(c, d, fr.x), fr.y);
}

// Un texel quelconque de l'atlas, filtré de la même façon bornée.
vec4 cube_atlas_lire(texture2D tex, int face, vec2 st) {
    float fc = cube_atlas_face(tex);
    vec2 p = (st + 1.0) * 0.5 * fc - 0.5;
    vec2 p0 = floor(p);
    vec2 fr = p - p0;

    vec4 a = texelFetch(tex, cube_atlas_texel(face, fc, p0, vec2(0.0, 0.0)), 0);
    vec4 b = texelFetch(tex, cube_atlas_texel(face, fc, p0, vec2(1.0, 0.0)), 0);
    vec4 c = texelFetch(tex, cube_atlas_texel(face, fc, p0, vec2(0.0, 1.0)), 0);
    vec4 d = texelFetch(tex, cube_atlas_texel(face, fc, p0, vec2(1.0, 1.0)), 0);

    return mix(mix(a, b, fr.x), mix(c, d, fr.x), fr.y);
}

#endif

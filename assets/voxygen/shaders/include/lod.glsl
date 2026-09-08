#ifndef LOD_GLSL
#define LOD_GLSL

#include <random.glsl>
#include <sky.glsl>
#include <srgb.glsl>
#include <cube.glsl>

layout(set = 0, binding = 7) uniform texture2D t_horizon;
layout(set = 0, binding = 8) uniform sampler s_horizon;


const float MIN_SHADOW = 0.33;

vec2 pos_to_tex(vec2 pos) {
    // Want: (pixel + 0.5)
    vec2 uv_pos = (focus_off.xy + pos + 16) / 32.0;
    return vec2(uv_pos.x, uv_pos.y);
}

// textureBicubic from https://stackoverflow.com/a/42179924
vec4 cubic(float v) {
    vec4 n = vec4(1.0, 2.0, 3.0, 4.0) - v;
    vec4 s = n * n * n;
    float x = s.x;
    float y = s.y - 4.0 * s.x;
    float z = s.z - 4.0 * s.y + 6.0 * s.x;
    float w = 6.0 - x - y - z;
    return vec4(x, y, z, w) * (1.0/6.0);
}

// Computes atan(y, x), except with more stability when x is near 0.
float atan2(in float y, in float x) {
    bool s = (abs(x) > abs(y));
    return mix(PI/2.0 - atan(x,y), atan(y,x), s);
}

// NOTE: We assume the sampled coordinates are already in "texture pixels".
vec4 textureBicubic(texture2D tex, sampler sampl, vec2 texCoords) {
    // TODO: remove all textureSize calls and replace with constants
   vec2 texSize = textureSize(sampler2D(tex, sampl), 0);
   vec2 invTexSize = 1.0 / texSize;

   texCoords = texCoords/* * texSize */ - 0.5;


    vec2 fxy = fract(texCoords);
    texCoords -= fxy;

    vec4 xcubic = cubic(fxy.x);
    vec4 ycubic = cubic(fxy.y);

    vec4 c = texCoords.xxyy + vec2 (-0.5, +1.5).xyxy;

    vec4 s = vec4(xcubic.xz + xcubic.yw, ycubic.xz + ycubic.yw);
    vec4 offset = c + vec4 (xcubic.yw, ycubic.yw) / s;

    offset *= invTexSize.xxyy;

    vec4 sample0 = texture(sampler2D(tex, sampl), offset.xz);
    vec4 sample1 = texture(sampler2D(tex, sampl), offset.yz);
    vec4 sample2 = texture(sampler2D(tex, sampl), offset.xw);
    vec4 sample3 = texture(sampler2D(tex, sampl), offset.yw);

    float sx = s.x / (s.x + s.y);
    float sy = s.z / (s.z + s.w);

    return mix(
       mix(sample3, sample2, sx), mix(sample1, sample0, sx)
    , sy);
}

vec4 textureMaybeBicubic(texture2D tex, sampler sampl, vec2 texCoords) {
    // TODO: Allow regular `texture` to be used when cause of light leaking issues is found
    //#if (CLOUD_MODE >= CLOUD_MODE_HIGH)
        return textureBicubic(tex, sampl, texCoords);
    //#else
    //    vec2 offset = (texCoords + vec2(-1.0, 0.5)) / textureSize(sampler2D(tex, sampl), 0);
    //    return texture(sampler2D(tex, sampl), offset);
    //#endif
}

// 16 bit version (each of the 2 8-bit components are combined after bilinear sampling)
// NOTE: We assume the sampled coordinates are already in "texture pixels".
vec2 textureBicubic16(texture2D tex, sampler sampl, vec2 texCoords) {
   vec2 texSize = textureSize(sampler2D(tex, sampl), 0);
   vec2 invTexSize = 1.0 / texSize;

   texCoords = texCoords - 0.5;


    vec2 fxy = fract(texCoords);
    texCoords -= fxy;

    vec4 xcubic = cubic(fxy.x);
    vec4 ycubic = cubic(fxy.y);

    vec4 c = texCoords.xxyy + vec2 (-0.5, +1.5).xyxy;

    vec4 s = vec4(xcubic.xz + xcubic.yw, ycubic.xz + ycubic.yw);
    vec4 offset = c + vec4 (xcubic.yw, ycubic.yw) / s;

    offset *= invTexSize.xxyy;

    vec4 sample0_v4 = textureLod(sampler2D(tex, sampl), offset.xz, 0);
    vec4 sample1_v4 = textureLod(sampler2D(tex, sampl), offset.yz, 0);
    vec4 sample2_v4 = textureLod(sampler2D(tex, sampl), offset.xw, 0);
    vec4 sample3_v4 = textureLod(sampler2D(tex, sampl), offset.yw, 0);
    vec2 sample0 = sample0_v4.rb / 256.0 + sample0_v4.ga;
    vec2 sample1 = sample1_v4.rb / 256.0 + sample1_v4.ga;
    vec2 sample2 = sample2_v4.rb / 256.0 + sample2_v4.ga;
    vec2 sample3 = sample3_v4.rb / 256.0 + sample3_v4.ga;

    float sx = s.x / (s.x + s.y);
    float sy = s.z / (s.z + s.w);

    return mix(mix(sample3, sample2, sx), mix(sample1, sample0, sx), sy);
}

// Gets the altitude at a position relative to focus_off.
float alt_at(vec2 pos) {
    vec4 alt_sample = textureLod(sampler2D(t_alt, s_alt), wpos_to_uv(focus_off.xy + pos), 0);
    return (((alt_sample.r * (1.0 / 256.0) + alt_sample.g) * view_distance.w) + view_distance.z - focus_off.z);
}

float alt_at_real(vec2 pos) {
    return ((textureBicubic16(t_alt, s_alt, pos_to_tex(pos)).r * view_distance.w) + view_distance.z - focus_off.z);
}


float horizon_at2(vec4 f_horizons, float alt, vec3 pos, vec4 light_dir) {
    const float PI_2 = 3.1415926535897932384626433832795 / 2.0;
    const float MIN_LIGHT = 0.0;
    
    vec2 f_horizon = mix(f_horizons.rg, f_horizons.ba, bvec2(light_dir.x < 0.0));
    float angle = tan(f_horizon.x * PI_2);
    float height = f_horizon.y * view_distance.w + view_distance.z;
    const float w = 0.1;
    float deltah = height - alt - focus_off.z;
    float lighta = -light_dir.z / max(abs(light_dir.x), 0.0001);
    // NOTE: Ideally, deltah <= 0.0 is a sign we have an oblique horizon angle.
    float deltax = deltah / max(angle, 0.0001);
    float lighty = lighta * deltax;
    float deltay = lighty - deltah + max(pos.z - alt, 0.0);
    // NOTE: the "real" deltah should always be >= 0, so we know we're only handling the 0 case with max.
    float s = mix(max(min(max(deltay, 0.0) / max(deltax, 0.0001) / w, 1.0), 0.0), 1.0, deltah <= 0);
    return max(s * s * (3.0 - 2.0 * s), MIN_LIGHT);
}

vec2 splay(vec2 pos) {
    vec2 scale = textureSize(sampler2D(t_alt, s_alt), 0) * 32.0;
    float lod_dist = view_distance.x * 0.95 / max(scale.x, scale.y);
    float dist = abs(pos.x) + abs(pos.y);
    float stretch = (pow(dist, 5.5) * 0.75 + dist * 0.25) * (1.0 - lod_dist) + lod_dist;
    vec2 splayed = pos * stretch * scale;
    if (abs(pos.x) > 0.99 || abs(pos.y) > 0.99) {
        splayed *= 50.0;
    }
    return splayed;
}

vec3 lod_norm(vec2 f_pos/*vec3 pos*/, vec4 square) {
    float altx0 = alt_at(vec2(square.x, f_pos.y));
    float altx1 = alt_at(vec2(square.z, f_pos.y));
    float alty0 = alt_at(vec2(f_pos.x, square.y));
    float alty1 = alt_at(vec2(f_pos.x, square.w));
    float slope = abs(altx1 - altx0) + abs(alty0 - alty1);

    vec3 norm = normalize(vec3(
        (altx0 - altx1) / (square.z - square.x),
        (alty0 - alty1) / (square.w - square.y),
        1.0
    ));

    return faceforward(norm, vec3(0.0, 0.0, -1.0), norm);
}

vec3 lod_norm(vec2 f_pos) {
    const float SAMPLE_W = 32;
    return lod_norm(f_pos, vec4(f_pos - vec2(SAMPLE_W), f_pos + vec2(SAMPLE_W)));
}


vec3 lod_pos(vec2 pos, vec2 focus_pos) {
    // Remove spiking by "pushing" vertices towards local optima
    vec2 delta = splay(pos);
    vec2 hpos = focus_pos + delta;

    vec2 dir = normalize(pos);
    float shift = 150.0 * pow(length(pos), 3.0);
    for (int i = 1; i < 10; i ++) {
        hpos -= dir * dot(normalize(lod_norm(hpos)).xy, dir) * shift / float(i);
    }

    return vec3(hpos, alt_at_real(hpos));
}

// --------------------------------------------------------------------------
// La nappe sur la planète (D27)
// --------------------------------------------------------------------------
//
// Rien de ce qui suit ne remplace ce qui précède : le monde plat garde ses
// fonctions mot pour mot, parce qu'il est l'oracle de non-régression de
// l'érosion (D37). Ce sont des branches, pas des réécritures.

// `splay`, sur une planète.
//
// Même courbe de densité — fine au pied du joueur, lâche au loin — mais deux
// changements de nature. L'échelle n'est plus la carte entière : c'est la
// **portée du lointain**, l'angle au-delà duquel plus rien n'est visible, et
// sur cette planète il vaut quelques milliers de blocs, pas trente mille. Et le
// bord ne part plus à cinquante fois la carte : il **ferme la calotte** à
// l'horizon, parce que le monde s'y arrête pour de bon.
//
// C'est le point où le balayage de rectangle disparaît. La grille reste un
// carré de paramètres, mais ce qu'elle paramètre est un disque géodésique
// centré sur le joueur — on marche sur la surface, on ne balaye plus le patron.
vec2 splay_cube(vec2 pos) {
    float scale = cube.x * cube_portee();
    float lod_dist = view_distance.x * 0.95 / scale;
    float dist = abs(pos.x) + abs(pos.y);
    float stretch = (pow(dist, 5.5) * 0.75 + dist * 0.25) * (1.0 - lod_dist) + lod_dist;
    vec2 splayed = pos * stretch * scale;
    float r = length(splayed);
    return r > scale ? splayed * (scale / r) : splayed;
}

// L'altitude **absolue** de la carte dans une direction du monde.
//
// À la différence d'`alt_at`, elle ne retire pas `focus_off.z` : on en a besoin
// telle quelle pour former `R + alt`, qui est une longueur comptée depuis le
// centre de la planète et non depuis le foyer. Le nom porte l'écart de
// convention, faute de quoi on se tromperait un jour de plusieurs milliers de
// blocs sans que rien ne le signale.
float cube_alt_at(vec3 dir) {
    vec2 st;
    int face = cube_face_de_direction(dir, st);
    return cube_atlas_alt16(t_alt, face, st) * view_distance.w + view_distance.z;
}

// L'altitude d'un point du plan tangent, par la carte exponentielle.
float cube_alt_de(vec2 d) {
    vec3 e, n;
    return cube_alt_at(cube_exp(d, e, n));
}

// La part horizontale de la normale, **dans le plan tangent** : ce que
// `lod_norm` rend sur une carte plate, transposé au chart.
vec2 cube_pente(vec2 d) {
    const float W = 32.0;
    float ax0 = cube_alt_de(d - vec2(W, 0.0));
    float ax1 = cube_alt_de(d + vec2(W, 0.0));
    float ay0 = cube_alt_de(d - vec2(0.0, W));
    float ay1 = cube_alt_de(d + vec2(0.0, W));
    return normalize(vec3((ax0 - ax1) / (2.0 * W), (ay0 - ay1) / (2.0 * W), 1.0)).xy;
}

// La borne de la boucle qui pousse un sommet vers l'optimum local, exclusive
// comme celle du monde plat : cinq tours ici, neuf là-bas.
//
// Le disque géodésique couvre quelques milliers de blocs là où le drap plat en
// couvrait trente mille : à nombre de sommets égal la trame y est **une dizaine
// de fois plus fine**, donc bien moins exposée aux pics qui ont fait écrire
// cette boucle. Et chaque tour coûte ici huit lectures de texel là où la version
// plate en payait une — c'est ce rapport, et non le nombre de tours, qui décide
// du budget.
const int CUBE_LOD_TOURS = 6;

// La position d'un sommet de la nappe, son repère et sa normale.
//
// Tout se passe dans le plan tangent au joueur : `d` est un déplacement en
// blocs, `length(d)` est la distance géodésique, et la carte exponentielle
// n'intervient qu'au moment de lire le monde. Aucune position du patron n'est
// jamais soustraite d'une autre — au-delà d'une couture, deux points voisins
// sont séparés d'une face entière de grille.
vec3 cube_lod_pos(vec2 pos, out vec3 f_norm, out float portee) {
    vec2 d = splay_cube(pos);

    // Le même lissage que `lod_pos`, mené dans le chart.
    vec2 sens = normalize(pos);
    float shift = 150.0 * pow(length(pos), 3.0);
    for (int i = 1; i < CUBE_LOD_TOURS; i++) {
        d -= sens * dot(cube_pente(d), sens) * shift / float(i);
    }

    portee = length(d);
    vec3 est_p, nord_p;
    vec3 dir = cube_exp(d, est_p, nord_p);

    // La normale, avec le repère analytique du point atteint : la verticale y
    // est `dir`, jamais `+Z`.
    //
    // La demi-largeur du stencil suit **l'écartement local de la trame**, comme
    // sur une carte plate où elle vient de `splay(v_pos ± dims)`. Une largeur
    // fixe sous-échantillonnerait au loin, là où deux sommets voisins sont
    // distants de centaines de blocs, et la nappe s'y couvrirait de bruit.
    vec2 dims = vec2(1.0 / view_distance.y);
    float w = max(0.5 * length(splay_cube(pos + dims) - splay_cube(pos - dims)), 1.0);
    float dx = (cube_alt_de(d + vec2(w, 0.0)) - cube_alt_de(d - vec2(w, 0.0))) / (2.0 * w);
    float dy = (cube_alt_de(d + vec2(0.0, w)) - cube_alt_de(d - vec2(0.0, w))) / (2.0 * w);
    f_norm = normalize(dir - est_p * dx - nord_p * dy);

    return dir;
}

// --------------------------------------------------------------------------
// Les enveloppes : lire la carte depuis une position de rendu
// --------------------------------------------------------------------------
//
// `alt_at` et `pos_to_tex` attendent une position **du patron**, décalée du
// foyer. Sur une planète, ce qu'un shader tient est une position **de rendu**,
// qui vit dans le repère 3D du cube : sur la face `+X`, son `.xy` mélange la
// verticale à l'horizontale. Ce n'est pas une erreur de flèche, c'est une
// rotation et un facteur d'échelle — et quatre faces sur six sont franchement
// fausses.
//
// D'où ces enveloppes. Elles branchent en interne, et la version plate y est
// l'ancienne expression, inchangée.

float alt_at_rendu(vec3 f_pos) {
    if (cube_actif()) {
        return cube_alt_at(cube_direction_de_rendu(f_pos)) - focus_off.z;
    }
    return alt_at(f_pos.xy);
}

vec4 horizon_rendu(vec3 f_pos) {
    if (cube_actif()) {
        vec2 st;
        int face = cube_face_de_direction(cube_direction_de_rendu(f_pos), st);
        return cube_atlas_lire(t_horizon, face, st);
    }
    return textureMaybeBicubic(t_horizon, s_horizon, pos_to_tex(f_pos.xy));
}


#ifdef HAS_LOD_FULL_INFO
layout(set = 0, binding = 10)
uniform texture2D t_map;
layout(set = 0, binding = 11)
uniform sampler s_map;

vec3 lod_col(vec2 pos) {
    #ifdef EXPERIMENTAL_PROCEDURALLODDETAIL
        vec2 wpos = pos + focus_off.xy;
        vec2 shift = vec2(
            textureLod(sampler2D(t_noise, s_noise), wpos / 200, 0).x - 0.5,
            textureLod(sampler2D(t_noise, s_noise), wpos / 200 + 0.5, 0).x - 0.5
        ) * 32 + vec2(
            textureLod(sampler2D(t_noise, s_noise), wpos / 50, 0).x - 0.5,
            textureLod(sampler2D(t_noise, s_noise), wpos / 50 + 0.5, 0).x - 0.5
        ) * 16;
        pos += shift;
        wpos += shift;
    #endif

    vec3 col = textureBicubic(t_map, s_map, pos_to_tex(pos)).rgb;

    return col;
}

// La couleur de la carte pour une position **de rendu**, bornée à la face.
//
// Le decalage procedural, quand il est allume, se prend dans le plan tangent :
// c'est deux lignes de moins que de le desactiver, et il garde son grain.
vec3 lod_col_rendu(vec3 f_pos) {
    if (!cube_actif()) {
        return lod_col(f_pos.xy);
    }
    vec3 dir = cube_direction_de_rendu(f_pos);
    #ifdef EXPERIMENTAL_PROCEDURALLODDETAIL
        vec2 wpos = cube_wpos_de_direction(dir);
        vec2 shift = vec2(
            textureLod(sampler2D(t_noise, s_noise), wpos / 200, 0).x - 0.5,
            textureLod(sampler2D(t_noise, s_noise), wpos / 200 + 0.5, 0).x - 0.5
        ) * 32 + vec2(
            textureLod(sampler2D(t_noise, s_noise), wpos / 50, 0).x - 0.5,
            textureLod(sampler2D(t_noise, s_noise), wpos / 50 + 0.5, 0).x - 0.5
        ) * 16;
        vec3 est_p, nord_p;
        vec3 haut = cube_haut();
        vec3 est = cube_repere.xyz;
        vec3 nord = cross(haut, est);
        dir = normalize(dir + est * shift.x / cube.x + nord * shift.y / cube.x);
    #endif
    vec2 st;
    int face = cube_face_de_direction(dir, st);
    return cube_atlas_lire(t_map, face, st).rgb;
}
#endif

vec3 water_diffuse(vec3 color, vec3 dir, float max_dist) {
    if (medium.x == 1) {
        float f_alt = alt_at_rendu(cam_pos.xyz);
        float fluid_alt = max(cam_pos.z + 1, floor(f_alt + 1));

        float water_dist = clamp((fluid_alt - cam_pos.z) / pow(max(dir.z, 0), 2), 0, max_dist);

        float fade = pow(0.95, water_dist);

        return mix(vec3(0.0, 0.2, 0.5)
            * (get_sun_brightness() * get_sun_color() + get_moon_brightness() * get_moon_color())
            * pow(0.99, max((fluid_alt - cam_pos.z) * 12.0 - dir.z * 200, 0)), color.rgb * exp(-MU_WATER * water_dist * 0.1), fade);
    } else {
        return color;
    }
}

void lod_voxels(vec3 f_pos, vec3 f_norm, vec3 cam_dir, out vec3 voxel_pos, out vec3 voxel_norm, out float voxel_sz, out float f_ao) {
    voxel_pos = f_pos;
    voxel_norm = f_norm;
    voxel_sz = 1.0;
    f_ao = 1.0;

    #ifndef EXPERIMENTAL_NOLODVOXELS
        #ifdef EXPERIMENTAL_PROCEDURALLODDETAIL
            const float MARCH_THRESHOLD = 4.0;
        #else
            const float MARCH_THRESHOLD = 2.0;
        #endif
        const float VOXEL_SCALE_FACTOR = 100000.0;

        // **Sur une planète, la marche se fait dans la carte géodésique.**
        //
        // Le réseau cubique de Veloren est aligné sur les axes du monde. Sur une
        // sphère il est donc oblique par rapport au sol dès qu'on quitte le
        // sommet d'une face, et il y ferait des dalles qui scintillent. On le
        // pose donc dans les **coordonnées normales** centrées sur le foyer :
        // `(x, y)` le déplacement du plan tangent rendu par `cube_log`, `z` la
        // hauteur au-dessus de la sphère.
        //
        // Ce chart est global sur toute la calotte et continu, ce qui est la
        // condition pour que le `floor` du réseau donne la même case à deux
        // fragments voisins. Un repère refait par sommet, lui, tournerait, et
        // c'est exactement ce qu'on cherche à éviter.
        if (cube_actif()) {
            vec3 dir = cube_direction_de_rendu(f_pos);
            vec3 est_p, nord_p;
            vec2 d = cube_log(dir);
            cube_exp(d, est_p, nord_p);

            // Le chart est une isométrie au point : la direction de vue s'y
            // pousse par simple projection sur le repère local.
            vec3 marche = normalize(vec3(
                dot(cam_dir, est_p),
                dot(cam_dir, nord_p),
                dot(cam_dir, dir)
            ));
            vec3 norme_l = vec3(dot(f_norm, est_p), dot(f_norm, nord_p), dot(f_norm, dir));
            vec3 wpos = vec3(d, cube_hauteur_rendu(f_pos));

            // La distance à la caméra est **géodésique** : la version plate
            // prend `distance(cam_pos.xy, f_pos.xy)`, qui ne veut rien dire ici.
            float portee = cube.x * acos(clamp(
                dot(cube_direction_de_rendu(cam_pos.xyz), dir), -1.0, 1.0
            ));
            voxel_sz = clamp(
                exp(floor(log(portee * 0.0001 + noise_2d(wpos.xy * 0.01) * 0.02) * 3) / 3)
                    * VOXEL_SCALE_FACTOR / (internal_res.x + internal_res.y),
                1.0, 128.0
            );

            float t = -MARCH_THRESHOLD * voxel_sz;
            int i = 0;
            while (t < MARCH_THRESHOLD * voxel_sz && i++ < 40) {
                vec3 deltas = (fract((wpos + marche * t) / voxel_sz) - step(vec3(0), marche * voxel_sz)) / -marche * voxel_sz;
                t += max(min(min(deltas.x, deltas.y), deltas.z), 0.001);

                vec3 centre = (floor((wpos + marche * t) / voxel_sz) + 0.5) * voxel_sz;
                float surf_depth = 0.0;
                #ifdef EXPERIMENTAL_PROCEDURALLODDETAIL
                    // `norme_l.z` est ce que `f_norm.z` était sur une carte
                    // plate : la part de la normale qui regarde le ciel.
                    surf_depth = (noise_3d(centre / voxel_sz * 0.01) - 0.5)
                        * 10.0
                        * voxel_sz
                        * pow(mix(0.0, mix(1.0, 0.0, max(norme_l.z, 0.0)), max(norme_l.z, 0.0)), 0.5);
                #endif
                if (dot(centre - wpos, -norme_l) > surf_depth) {
                    vec3 to_center = abs(centre - (wpos + marche * t));
                    vec3 n_l = step(max(max(to_center.x, to_center.y), to_center.z), to_center) * sign(-marche);
                    // La normale repart dans le monde : le réseau est local, ce
                    // qui l'éclaire ne l'est pas.
                    voxel_norm = normalize(est_p * n_l.x + nord_p * n_l.y + dir * n_l.z);
                    float dist = dot(marche * t, norme_l) + surf_depth;
                    f_ao = clamp(dist / voxel_sz + max(norme_l.z, 0.5), 0.25, 1.0);
                    // Et la position aussi : on repasse par la carte
                    // exponentielle, qui est l'inverse exact de `cube_log`.
                    vec3 e2, n2;
                    voxel_pos = cube_exp(centre.xy, e2, n2) * (cube.x + centre.z) - cube_origine.xyz;
                    return;
                }
            }
            voxel_pos = f_pos;
            vec3 n_l = step(max(max(norme_l.x, norme_l.y), norme_l.z), norme_l) * sign(-marche);
            voxel_norm = normalize(est_p * n_l.x + nord_p * n_l.y + dir * n_l.z);
            return;
        }

        vec3 wpos = f_pos + focus_off.xyz;

        voxel_sz = clamp(exp(floor(log(distance(cam_pos.xy, f_pos.xy) * 0.0001 + noise_2d(wpos.xy * 0.01) * 0.02) * 3) / 3) * VOXEL_SCALE_FACTOR / (internal_res.x + internal_res.y), 1.0, 128.0);

        float t = -MARCH_THRESHOLD * voxel_sz;
        int i = 0;
        while (t < MARCH_THRESHOLD * voxel_sz && i++<40) {
            vec3 deltas = (fract((wpos + cam_dir * t) / voxel_sz) - step(vec3(0), cam_dir * voxel_sz)) / -cam_dir * voxel_sz;
            t += max(min(min(deltas.x, deltas.y), deltas.z), 0.001);

            voxel_pos = (floor((wpos + cam_dir * t) / voxel_sz) + 0.5) * voxel_sz;
            float surf_depth = 0.0;
            #ifdef EXPERIMENTAL_PROCEDURALLODDETAIL
                surf_depth = (noise_3d(voxel_pos / voxel_sz * 0.01) - 0.5)
                    * 10.0
                    * voxel_sz
                    * pow(mix(0.0, mix(1.0, 0.0, max(f_norm.z, 0.0)), max(f_norm.z, 0.0)), 0.5);
            #endif
            if (dot(voxel_pos - wpos, -f_norm) > surf_depth) {
                vec3 to_center = abs(voxel_pos - (wpos + cam_dir * t));
                voxel_norm = step(max(max(to_center.x, to_center.y), to_center.z), to_center) * sign(-cam_dir);
                float dist = dot(cam_dir * t, f_norm) + surf_depth;
                f_ao = clamp(dist / voxel_sz + max(f_norm.z, 0.5), 0.25, 1.0);
                voxel_pos -= focus_off.xyz;
                return;
            }
        }
        voxel_pos = f_pos;
        // Fallback, if we didn't hit any voxels
        voxel_norm = step(max(max(f_norm.x, f_norm.y), f_norm.z), f_norm) * sign(-cam_dir);
    #endif
}

#endif

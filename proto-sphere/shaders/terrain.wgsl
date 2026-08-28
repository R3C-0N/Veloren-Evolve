// Terrain : la grille reste plate, la géométrie du rendu est la vraie sphère
// du cube (D27).
//
// Il n'y a plus de plan déroulé. Chaque chunk est dessiné une fois, à sa place
// sur la planète : ses coordonnées de face passent par la même projection
// cube → sphère que `cube::direction` côté CPU, à l'identique. C'est ce qui
// supprime les fausses adjacences — un chunk n'est jamais placé ailleurs qu'où
// il est.
//
// Ce que ça coûte : la rondeur n'est plus un réglage, c'est la taille du monde.

struct Globaux {
    vue_projection: mat4x4<f32>,
    // xyz = position 3D de la caméra
    camera: vec4<f32>,
    // x = début brouillard · y = fin · z = teinte des chunks
    // w = régime : 0 = la sphère du cube · 1 = la poche, plate
    params: vec4<f32>,
    // x = arête d'une face en blocs · y = rayon de rendu · z = côté de la table
    planete: vec4<f32>,
    ciel: vec4<f32>,
    // Plan de coupe, en (normale, distance) : on garde ce qui vérifie
    // dot(n, p) + d >= 0. Tout à zéro = on ne coupe rien.
    //
    // Il sert au portail. La caméra qui peint le passé pour la nappe se trouve
    // *derrière* le portail de sortie : sans coupe, elle a le mur d'enceinte
    // dans le nez et la nappe est grise.
    //
    // La coupe est calculée au sommet et interpolée. Ce n'est pas une
    // approximation : l'interpolation à correction de perspective reconstruit
    // exactement une fonction affine de la position du monde, donc la frontière
    // tombe au bon pixel.
    coupe: vec4<f32>,
};

struct Chunk {
    base_r: vec4<f32>,
    base_h: vec4<f32>,
    base_n: vec4<f32>,
    // xy = coin du chunk en coordonnées de face · z = altitude ajoutée
    origine: vec4<f32>,
    teinte: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globaux;
@group(0) @binding(1) var conforme: texture_2d<f32>;
@group(1) @binding(0) var<uniform> bloc: Chunk;

struct Sortie {
    @builtin(position) position: vec4<f32>,
    @location(0) couleur: vec3<f32>,
    @location(1) distance: f32,
    @location(2) coupe: f32,
};

// La table de projection, lue exactement comme le CPU la lit : mêmes
// échantillons, même interpolation bilinéaire. Si les deux divergeaient d'un
// texel, la visée se remettrait à dériver.
//
// `textureLoad` plutôt qu'un échantillonneur : une texture 32 bits flottante
// n'est pas filtrable partout, et on ne veut de toute façon rien d'autre
// qu'une bilinéaire écrite à la main.
fn tangent_de(s: f32, t: f32) -> vec2<f32> {
    let n = g.planete.z - 1.0;
    let x = clamp((s + 1.0) * 0.5 * n, 0.0, n - 0.0001);
    let y = clamp((t + 1.0) * 0.5 * n, 0.0, n - 0.0001);
    let i = i32(floor(x));
    let j = i32(floor(y));
    let fx = x - floor(x);
    let fy = y - floor(y);

    let a = textureLoad(conforme, vec2<i32>(i, j), 0).xy;
    let b = textureLoad(conforme, vec2<i32>(i + 1, j), 0).xy;
    let c = textureLoad(conforme, vec2<i32>(i, j + 1), 0).xy;
    let d = textureLoad(conforme, vec2<i32>(i + 1, j + 1), 0).xy;
    return mix(mix(a, b, fx), mix(c, d, fx), fy);
}

@vertex
fn vs_main(@location(0) local: vec3<f32>, @location(1) couleur: vec3<f32>) -> Sortie {
    let arete = g.planete.x;
    let rayon = g.planete.y;

    let u = bloc.origine.x + local.x;
    let v = bloc.origine.y + local.y;

    // La caméra est ramenée à l'origine : les positions valent quelques
    // milliers, les écarts quelques centaines, et f32 suffit largement.
    var monde: vec3<f32>;

    if (g.params.w > 0.5) {
        // La poche. Pas de projection du tout : le monde *est* le plan, et la
        // verticale est +Z. Une vraie branche, pas un rayon gonflé — gonfler
        // le rayon ment sur la taille des blocs, ce que le programme ne
        // s'autorise qu'au curseur « aplatir », et un plat approché ne se
        // mesure pas.
        monde = vec3<f32>(u, v, bloc.origine.z + local.z);
    } else {
        // La table donne les coordonnées du plan tangent gnomonique ; la
        // direction s'en tire d'une normalisation.
        let ab = tangent_de(2.0 * u / arete - 1.0, 2.0 * v / arete - 1.0);
        let dir = normalize(
            bloc.base_n.xyz + bloc.base_r.xyz * ab.x + bloc.base_h.xyz * ab.y,
        );
        monde = dir * (rayon + bloc.origine.z + local.z);
    }
    let p = monde - g.camera.xyz;

    var out: Sortie;
    out.position = g.vue_projection * vec4<f32>(p, 1.0);
    out.couleur = mix(couleur, couleur * bloc.teinte.rgb, g.params.z);
    out.distance = length(p);
    out.coupe = dot(g.coupe.xyz, monde) + g.coupe.w;
    return out;
}

@fragment
fn fs_main(entree: Sortie) -> @location(0) vec4<f32> {
    if (entree.coupe < 0.0) {
        discard;
    }
    let brume = clamp(
        (entree.distance - g.params.x) / max(g.params.y - g.params.x, 1.0),
        0.0,
        1.0,
    );
    return vec4<f32>(mix(entree.couleur, g.ciel.rgb, brume), 1.0);
}

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
    // x = début brouillard · y = fin · z = teinte des chunks · w = libre
    params: vec4<f32>,
    // x = arête d'une face en blocs · y = rayon de rendu · z = côté de la table
    planete: vec4<f32>,
    ciel: vec4<f32>,
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
};

/// La table conforme, lue exactement comme le CPU la lit : même échantillons,
/// même interpolation bilinéaire. Si les deux divergeaient d'un texel, la
/// visée se remettrait à dériver.
///
/// `textureLoad` plutôt qu'un échantillonneur : une texture 32 bits flottante
/// n'est pas filtrable partout, et on ne veut de toute façon rien d'autre
/// qu'une bilinéaire écrite à la main.
fn zeta_de(s: f32, t: f32) -> vec2<f32> {
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

    // Projection conforme : la case reste carrée, c'est sa taille qui varie.
    // `zeta` est la coordonnée stéréographique dans le repère de la face ; la
    // stéréographique inverse la ramène sur la sphère.
    let z = zeta_de(2.0 * u / arete - 1.0, 2.0 * v / arete - 1.0);
    let n2 = 1.0 + z.x * z.x + z.y * z.y;
    let locale = vec3<f32>(2.0 * z.x, 2.0 * z.y, 1.0 - z.x * z.x - z.y * z.y) / n2;

    let dir = normalize(
        bloc.base_r.xyz * locale.x + bloc.base_h.xyz * locale.y
            + bloc.base_n.xyz * locale.z,
    );

    // La caméra est ramenée à l'origine : les positions valent quelques
    // milliers, les écarts quelques centaines, et f32 suffit largement.
    let p = dir * (rayon + bloc.origine.z + local.z) - g.camera.xyz;

    var out: Sortie;
    out.position = g.vue_projection * vec4<f32>(p, 1.0);
    out.couleur = mix(couleur, couleur * bloc.teinte.rgb, g.params.z);
    out.distance = length(p);
    return out;
}

@fragment
fn fs_main(entree: Sortie) -> @location(0) vec4<f32> {
    let brume = clamp(
        (entree.distance - g.params.x) / max(g.params.y - g.params.x, 1.0),
        0.0,
        1.0,
    );
    return vec4<f32>(mix(entree.couleur, g.ciel.rgb, brume), 1.0);
}

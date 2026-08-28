// La nappe du portail : la fenêtre elle-même.
//
// Le sommet est celui du terrain, à l'identique — la nappe se tient sur la
// sphère, au même endroit et par la même projection que son cadre. Si elle
// suivait un autre chemin, elle se décalerait du cadre exactement là où la
// projection travaille le plus, c'est-à-dire près d'un coin.
//
// Le fragment, lui, ne calcule rien : il va chercher le pixel que le passé a
// déjà peint dans les coulisses, **au même endroit de l'écran**. C'est ce qui
// fait la fenêtre : les deux mondes sont rendus depuis deux caméras liées par
// la transformation du portail, donc les deux images se recouvrent pixel pour
// pixel, et il suffit de découper l'une dans l'autre.
//
// `textureLoad` plutôt qu'un échantillonneur : la coordonnée voulue est celle
// du fragment courant, en pixels entiers. Il n'y a rien à filtrer, et filtrer
// introduirait un demi-pixel de biais.

struct Globaux {
    vue_projection: mat4x4<f32>,
    camera: vec4<f32>,
    params: vec4<f32>,
    planete: vec4<f32>,
    ciel: vec4<f32>,
    coupe: vec4<f32>,
};

struct Chunk {
    base_r: vec4<f32>,
    base_h: vec4<f32>,
    base_n: vec4<f32>,
    origine: vec4<f32>,
    teinte: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globaux;
@group(0) @binding(1) var conforme: texture_2d<f32>;
@group(1) @binding(0) var<uniform> bloc: Chunk;
@group(2) @binding(0) var coulisses: texture_2d<f32>;

struct Sortie {
    @builtin(position) position: vec4<f32>,
    @location(0) distance: f32,
};

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

    // Les deux régimes, comme le terrain — et pour la même raison. Une nappe se
    // tient dans les deux mondes : celle de l'entrée sur la sphère, celle du
    // retour dans la poche. Sans cette branche, la seconde se fait projeter sur
    // une sphère de rayon nul et atterrit à côté du monde.
    var monde: vec3<f32>;
    if (g.params.w > 0.5) {
        monde = vec3<f32>(u, v, bloc.origine.z + local.z);
    } else {
        let ab = tangent_de(2.0 * u / arete - 1.0, 2.0 * v / arete - 1.0);
        let dir = normalize(
            bloc.base_n.xyz + bloc.base_r.xyz * ab.x + bloc.base_h.xyz * ab.y,
        );
        monde = dir * (rayon + bloc.origine.z + local.z);
    }
    let p = monde - g.camera.xyz;

    var out: Sortie;
    out.position = g.vue_projection * vec4<f32>(p, 1.0);
    out.distance = length(p);
    return out;
}

@fragment
fn fs_main(entree: Sortie) -> @location(0) vec4<f32> {
    let vu = textureLoad(coulisses, vec2<i32>(entree.position.xy), 0);

    // La brume du présent s'applique aussi à la fenêtre. Sans elle, un portail
    // lointain resterait net au milieu d'un paysage qui s'efface, et se
    // détacherait comme un autocollant collé sur l'image.
    let brume = clamp(
        (entree.distance - g.params.x) / max(g.params.y - g.params.x, 1.0),
        0.0,
        1.0,
    );
    return vec4<f32>(mix(vu.rgb, g.ciel.rgb, brume), 1.0);
}

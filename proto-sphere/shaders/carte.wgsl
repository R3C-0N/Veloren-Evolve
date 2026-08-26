// Vue 2D : le monde entier de dessus, avec ses coutures tracees.
//
// C'est ici qu'on juge la topologie. Un relief qui se coupe net sur une couture
// se voit immediatement ; un anneau d'ocean sur le pourtour signalerait qu'un
// bord de carte s'est reintroduit.

struct Carte {
    // xy = coin bas-gauche en NDC · zw = coin haut-droit
    cadre: vec4<f32>,
    // xy = position de la camera en uv · z = epaisseur des traits en uv · w = inutilise
    camera: vec4<f32>,
};

@group(0) @binding(0) var<uniform> c: Carte;
@group(1) @binding(0) var texture_carte: texture_2d<f32>;
@group(1) @binding(1) var echantillonneur: sampler;

struct Sortie {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> Sortie {
    var coins = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let uv = coins[i];
    let ndc = mix(c.cadre.xy, c.cadre.zw, uv);

    var out: Sortie;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    return out;
}

@fragment
fn fs_main(entree: Sortie) -> @location(0) vec4<f32> {
    var couleur = textureSample(texture_carte, echantillonneur, entree.uv).rgb;
    let e = c.camera.z;
    let uv = entree.uv;

    // Couture est-ouest : les deux bords verticaux sont le meme meridien.
    if (uv.x < e || uv.x > 1.0 - e) {
        couleur = mix(couleur, vec3<f32>(1.0, 0.85, 0.2), 0.85);
    }
    // Poles : les bords horizontaux se recollent sur eux-memes, decales.
    if (uv.y < e || uv.y > 1.0 - e) {
        couleur = mix(couleur, vec3<f32>(0.4, 0.8, 1.0), 0.85);
    }
    // Le meridien d'une demi-largeur : la ou ressort qui franchit un pole.
    if (abs(uv.x - 0.5) < e * 0.6 && fract(uv.y * 60.0) < 0.5) {
        couleur = mix(couleur, vec3<f32>(0.4, 0.8, 1.0), 0.6);
    }

    // Marqueur de la camera.
    let d = abs(uv - c.camera.xy);
    if ((d.x < e * 4.0 && d.y < e * 1.0) || (d.y < e * 4.0 && d.x < e * 1.0)) {
        couleur = vec3<f32>(1.0, 0.2, 0.2);
    }

    return vec4<f32>(couleur, 1.0);
}

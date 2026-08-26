// Vue 2D : le patron du cube, avec ses recollements appariés par la couleur.
//
// Presque tout est peint dans la texture côté CPU, où la disposition du patron
// est connue. Il ne reste ici que le marqueur de caméra.

struct Carte {
    // xy = coin bas-gauche en NDC · zw = coin haut-droit
    cadre: vec4<f32>,
    // xy = position de la caméra en uv · z = un texel en uv · w = inutilisé
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
    let d = abs(entree.uv - c.camera.xy);
    if ((d.x < e * 5.0 && d.y < e * 1.2) || (d.y < e * 5.0 && d.x < e * 1.2)) {
        couleur = vec3<f32>(1.0, 0.15, 0.15);
    }

    return vec4<f32>(couleur, 1.0);
}

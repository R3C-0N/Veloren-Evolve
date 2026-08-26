// Terrain : la grille reste plate, seul le rendu se courbe (D27).
//
// Les positions arrivent locales au chunk ; `bloc.decalage` porte la position
// du chunk RELATIVE A LA CAMERA, deja repliee par le CPU. La camera est donc a
// l'origine, ce qui evite les pertes de precision et rend les coutures
// invisibles : un chunk d'en face arrive avec un petit decalage, pas avec la
// largeur du monde.

struct Globaux {
    vue_projection: mat4x4<f32>,
    // x = rayon de courbure (0 = plat) · y = debut brouillard · z = fin · w = teinte chunks
    params: vec4<f32>,
    ciel: vec4<f32>,
};

struct Chunk {
    // xyz = decalage relatif a la camera · w = sens de l'axe Y (-1 = chunk replie
    // au-dela d'un pole, son maillage est reflechi)
    decalage: vec4<f32>,
    teinte: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globaux;
@group(1) @binding(0) var<uniform> bloc: Chunk;

struct Sortie {
    @builtin(position) position: vec4<f32>,
    @location(0) couleur: vec3<f32>,
    @location(1) distance: f32,
};

@vertex
fn vs_main(@location(0) local: vec3<f32>, @location(1) couleur: vec3<f32>) -> Sortie {
    let rel = vec3<f32>(
        bloc.decalage.x + local.x,
        bloc.decalage.y + bloc.decalage.w * local.y,
        bloc.decalage.z + local.z,
    );

    // La courbure, et rien d'autre. Aucune position lue ici ne redescend vers
    // le CPU : le raycast et la selection de bloc travaillent a plat.
    let rayon = g.params.x;
    var p = rel;
    let d = length(rel.xy);
    if (rayon > 0.0) {
        let dd = min(d, rayon);
        p.z = p.z - (rayon - sqrt(max(rayon * rayon - dd * dd, 0.0)));
    }

    var out: Sortie;
    out.position = g.vue_projection * vec4<f32>(p, 1.0);
    out.couleur = mix(couleur, couleur * bloc.teinte.rgb, g.params.w);
    out.distance = d;
    return out;
}

@fragment
fn fs_main(entree: Sortie) -> @location(0) vec4<f32> {
    let brume = clamp(
        (entree.distance - g.params.y) / max(g.params.z - g.params.y, 1.0),
        0.0,
        1.0,
    );
    return vec4<f32>(mix(entree.couleur, g.ciel.rgb, brume), 1.0);
}

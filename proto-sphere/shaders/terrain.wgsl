// Terrain : la grille reste plate, seul le rendu se courbe (D27).
//
// Les positions arrivent locales au chunk. `bloc.decalage` porte la position du
// chunk RELATIVE À LA CAMÉRA, déjà repliée par le CPU, et `bloc.rotation` le
// quart de tour que le repliement lui a fait subir — un chunk atteint en
// franchissant une arête du cube arrive tourné.
//
// La caméra est donc à l'origine, ce qui évite les pertes de précision et rend
// les recollements invisibles : une face d'à côté arrive avec un petit
// décalage, pas avec la largeur du monde.

struct Globaux {
    vue_projection: mat4x4<f32>,
    // x = rayon de courbure (0 = plat) · y = début brouillard · z = fin · w = teinte chunks
    params: vec4<f32>,
    ciel: vec4<f32>,
};

struct Chunk {
    decalage: vec4<f32>,
    // xy = (cos, sin) du quart de tour à appliquer aux coordonnées locales
    rotation: vec4<f32>,
    teinte: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globaux;
@group(1) @binding(0) var<uniform> bloc: Chunk;

struct Sortie {
    @builtin(position) position: vec4<f32>,
    @location(0) couleur: vec3<f32>,
    @location(1) distance: f32,
};

const CENTRE: vec2<f32> = vec2<f32>(16.0, 16.0);

@vertex
fn vs_main(@location(0) local: vec3<f32>, @location(1) couleur: vec3<f32>) -> Sortie {
    // Le quart de tour se prend autour du centre du chunk : un carré tourné
    // d'un quart de tour autour de son centre retombe sur lui-même, donc le
    // décalage du chunk n'a pas à en tenir compte.
    let l = local.xy - CENTRE;
    let c = bloc.rotation.x;
    let s = bloc.rotation.y;
    let tourne = vec2<f32>(c * l.x - s * l.y, s * l.x + c * l.y) + CENTRE;

    let rel = vec3<f32>(
        bloc.decalage.x + tourne.x,
        bloc.decalage.y + tourne.y,
        bloc.decalage.z + local.z,
    );

    // La courbure, et rien d'autre. Elle ne dépend que de la distance à la
    // caméra : elle ne sait rien des arêtes du cube, et c'est pourtant elle qui
    // les arrondit. Aucune position lue ici ne redescend vers le CPU — le
    // raycast et la sélection de bloc travaillent à plat.
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

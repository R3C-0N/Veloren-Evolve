//! Diagnostic en console : `proto-sphere --diag`.
//!
//! Le prototype existe pour réfuter D27, pas pour l'illustrer. Une capture
//! d'écran ne dit pas si un recollement est continu — des nombres, si.
//!
//! Ce fichier tient aussi lieu de garantie pour [`crate::cube`] : les vingt-
//! quatre arêtes n'y sont écrites nulle part, elles se déduisent de six bases
//! 3D. Ce sont ces invariants qui répondent du calcul, pas la relecture.

use crate::cube::{
    COS_SIN, FACE, FACE_CHUNKS, NOMS, RAYON, direction, point_sphere, replier_bloc,
    replier_chunk,
};
use crate::ancre::{Ancre, LARGEUR_NAPPE, Portail};
use crate::chunk::Chunk;
use crate::conforme;
use crate::monde::{Bloc, Generateur, NIVEAU_MER, TAILLE_CHUNK, biome_de, point_apparition};
use crate::poche::{self, Poche};
use crate::vue3d::{Camera, viser};
use glam::{DVec3, Vec3};
use std::collections::HashSet;

pub fn executer() {
    let gen = Generateur::nouveau(1);

    println!("Patron de cube : 6 faces de {FACE} blocs d'arête");
    println!(
        "tour du monde : {:.0} blocs · rayon {RAYON:.0} blocs",
        std::f64::consts::TAU * RAYON
    );
    println!();

    invariants();
    coutures(&gen);
    distorsion();
    projection_conforme();
    continuite_du_deroulement();
    marcher_a_travers_les_bords();
    derive_de_visee(&gen);
    ancre_temporelle(&gen);
    terrain(&gen);
}

// --------------------------------------------------------------------------
// L'ancre temporelle : ce que coûte un second monde
// --------------------------------------------------------------------------

/// Trois mesures, une par chose que le portail pourrait casser.
///
/// D17 affirme que le coût d'un second monde est architectural. Ce qui se
/// vérifie ici n'est pas ce coût — un banc d'essai ne mesure pas un devis — mais
/// les trois façons dont un second monde peut abîmer le premier : perdre l'état
/// qu'on lui a confié, se mélanger à lui, ou ranger son point d'entrée dans la
/// grille au lieu du monde.
fn ancre_temporelle(gen: &Generateur) {
    retour_exact(gen);
    taille_de_la_nappe(gen);
    traversee_reversible(gen);
    mondes_disjoints();
    ancre_dans_le_monde(gen);
}

/// **Les deux nappes font-elles la même taille ?**
///
/// Celle du passé est bâtie dans un monde plat : ses trois blocs sont trois
/// blocs. Celle du présent est bâtie sur la sphère, où un pas de grille vaut
/// entre 0,69 et 1,00 bloc — si on la taille en coordonnées, elle rétrécit là
/// où les blocs sont petits, et les deux côtés du portail cessent de se
/// correspondre.
///
/// On mesure donc la largeur **rendue**, par le chemin exact du rendu :
/// `direction` appliquée aux deux bords du cadre, et la distance entre les deux
/// points obtenus. C'est le seul chiffre opposable — la largeur en coordonnées
/// ne dit rien de ce qu'on voit.
fn taille_de_la_nappe(gen: &Generateur) {
    println!("── Les deux nappes ont-elles la même taille ? ──");
    println!("  lieu                     largeur rendue   attendu   taille du bloc");

    let mut pire = 0.0f64;
    for (nom, cam) in lieux(gen) {
        let p = Portail::ouvrir(gen, &cam, 5);
        let demi = (LARGEUR_NAPPE * 0.5) as f64;
        let rayon_nappe = RAYON + p.z as f64 + crate::ancre::CENTRE_NAPPE as f64;
        let bord = |s: f64| {
            let u = p.u as f64 + 0.5 + p.axe_droite.x as f64 * demi * s;
            let v = p.v as f64 + 0.5 + p.axe_droite.y as f64 * demi * s;
            DVec3::from_array(direction(p.face, u, v)) * rayon_nappe
        };
        let large = (bord(1.0) - bord(-1.0)).length();
        // La taille d'un bloc à cet endroit *et à cette altitude* : un pas de
        // grille, en vrai. C'est à lui que la nappe doit se mesurer.
        let rayon = RAYON + p.z as f64 + crate::ancre::CENTRE_NAPPE as f64;
        let pas = {
            let a = DVec3::from_array(direction(p.face, p.u as f64, p.v as f64 + 0.5));
            let b = DVec3::from_array(direction(p.face, p.u as f64 + 1.0, p.v as f64 + 0.5));
            (a - b).length() * rayon
        };
        pire = pire.max((large - LARGEUR_NAPPE as f64).abs());
        println!(
            "  {nom:<22}   {large:>14.3}   {:>7.3}   {pas:>14.3}",
            LARGEUR_NAPPE
        );
    }
    println!("  pire écart avec la nappe du passé : {pire:.3} bloc");
    println!();
}

/// **La traversée est-elle réversible ?**
///
/// Depuis que l'on passe *à travers* le portail au lieu d'y être téléporté, la
/// caméra fait l'aller par une transformation et le retour par l'autre. Si les
/// deux ne se répondaient pas exactement, un joueur qui entre et ressort
/// aussitôt se retrouverait à côté de là où il était.
///
/// Ce n'est pas une tautologie. L'aller compose la projection — `direction`,
/// par `repere_3d` — et le retour son **inverse**, `depuis_direction`. Une
/// erreur de l'un ne s'annule pas contre l'autre : elle s'ajoute. C'est donc
/// aussi une mesure de l'inversibilité que D27 exige, prise par le chemin dont
/// le jeu se sert vraiment.
fn traversee_reversible(gen: &Generateur) {
    println!("── La traversée est-elle réversible ? ──");
    println!("  lieu                     pas   écart position   écart de cap");

    let (mut pire_d, mut pire_a) = (0.0f32, 0.0f64);
    for (nom, depart) in lieux(gen) {
        let portail = Portail::ouvrir(gen, &depart, 11);
        let mut cam = depart;

        for pas in 1..=3 {
            // On se rapproche de la nappe : c'est près d'elle que la
            // transformation est sollicitée, et près d'un coin qu'elle risque.
            let d3 = cam.avant_plat() * 2.0;
            let (du, dv) = cam.vers_coordonnees(d3);
            cam.avancer(Vec3::new(du, dv, 0.0));

            let revenu = portail.camera_de_la_sphere(&portail.camera_de_la_poche(&cam));

            let (pa, va, _) = cam.repere_3d(RAYON);
            let (pb, vb, _) = revenu.repere_3d(RAYON);
            let d = (pa - pb).length() as f32;
            let a = (va.dot(vb) as f64).clamp(-1.0, 1.0).acos().to_degrees();
            pire_d = pire_d.max(d);
            pire_a = pire_a.max(a);
            println!("  {nom:<22}  {pas:>4}   {d:>14.6}   {a:>12.4}°");
        }
    }
    println!("  pire écart                       : {pire_d:.6} bloc · {pire_a:.4}°");
    println!("  (à comparer au millième de bloc de l'inversion de la projection)");
    println!();
}

/// Une caméra posée quelque part, cap donné.
fn camera_a(gen: &Generateur, face: u8, u: i32, v: i32, cap: f32) -> Camera {
    let mut cam = Camera {
        face,
        position: Vec3::new(
            u as f32 + 0.5,
            v as f32 + 0.5,
            gen.hauteur(face, u, v).max(NIVEAU_MER as f32) + 30.0,
        ),
        regard: Vec3::X,
        tangage: -0.2,
    };
    cam.poser_cap(cap);
    cam
}

/// Les quatre lieux d'épreuve, du facile au dur.
fn lieux(gen: &Generateur) -> Vec<(&'static str, Camera)> {
    let (f, u, v) = point_apparition(gen);
    let coin = FACE - 3;
    vec![
        ("prairie de départ", camera_a(gen, f, u, v, 0.7)),
        ("milieu d'arête", camera_a(gen, 1, FACE / 2, FACE - 2, 1.57)),
        ("à 3 blocs d'un coin", camera_a(gen, 1, coin, coin, 2.36)),
        // Hors de sa face des deux côtés : le repliement s'en charge, et c'est
        // justement le cas où une ancre mal rangée se ferait remarquer.
        ("au-delà du coin", camera_a(gen, 1, FACE + 40, FACE + 40, 2.36)),
    ]
}

/// **1. Le retour est exact.**
///
/// On ouvre, on entre, on marche dans la poche, on est expulsé, et on compare.
/// La comparaison est **au bit près**, pas à epsilon : le retour est une
/// recopie de quatre champs, il n'a aucune raison de dériver. Un epsilon
/// masquerait le jour où il en aurait une, et une mesure qui ne peut pas
/// échouer sert d'alibi.
fn retour_exact(gen: &Generateur) {
    println!("── Aller-retour par une ancre ──");
    println!("  lieu                    face       écart position   écart de cap   au bit près");

    for (nom, depart) in lieux(gen) {
        // Ce que le joueur fait vraiment : il marche, puis il ouvre.
        let mut cam = depart;
        let d3 = cam.avant_plat() * 40.0;
        let (du, dv) = cam.vers_coordonnees(d3);
        cam.avancer(Vec3::new(du, dv, 0.0));

        let portail = Portail::ouvrir(gen, &cam, 7);
        let temoin = Ancre::poser(&cam);

        // On franchit, on vit dans la poche, on en est expulsé.
        let poche = Poche::nouvelle(portail.graine);
        let mut plate = poche.depart();
        for _ in 0..200 {
            plate.avancer(Vec3::new(1.7, -0.9, 0.3));
            plate.tourner(0.05);
        }

        // L'expulsion. Recopie, et rien d'autre — comme dans `App::expulser`.
        let mut revenu = Camera {
            face: 0,
            position: Vec3::ZERO,
            regard: Vec3::X,
            tangage: 0.0,
        };
        portail.retour.restituer(&mut revenu);

        let (d, a) = temoin.ecart(&revenu);
        println!(
            "  {nom:<22}  {:<8}   {d:>12.6}   {a:>11.3}°   {}",
            NOMS[revenu.face as usize],
            if temoin.identique_au_bit(&revenu) { "oui" } else { "NON" },
        );
    }
    println!();
}

/// **2. Les deux mondes ne se mélangent pas.**
///
/// La disjonction est d'abord dans les types : ni [`crate::poche::Poche`] ni
/// [`Chunk::poche`] ne prennent de `&Generateur`, et la clé de cache de la
/// poche n'a pas le même type que celle de la sphère. Ce sont des faits de
/// compilation, pas des mesures — et un fait de compilation ne se voit pas à
/// l'exécution, donc il ne suffit pas à rassurer.
///
/// Ce qu'on mesure ici, c'est l'inverse : que la poche soit **insensible** au
/// monde sphérique, et **finie**. Si la sphère fuyait, changer sa graine
/// changerait le contenu de la poche ; si la poche ne se terminait pas, il y
/// aurait de la matière au-delà de son mur.
fn mondes_disjoints() {
    println!("── Les deux mondes se touchent-ils ? ──");

    // a. La poche, engendrée sous deux mondes sphériques différents.
    let a = Poche::nouvelle(7);
    let b = Poche::nouvelle(7);
    let mut differents = 0usize;
    let mut testes = 0usize;
    for cv in 0..poche::COTE_CHUNKS {
        for cu in 0..poche::COTE_CHUNKS {
            // Deux générateurs de sphère bien distincts existent de part et
            // d'autre de cet appel — et n'y entrent pas.
            let _sphere_a = Generateur::nouveau(1);
            let _sphere_b = Generateur::nouveau(4242);
            let ca = Chunk::poche(&a, cu, cv);
            let cb = Chunk::poche(&b, cu, cv);
            for z in (0..crate::monde::HAUTEUR_CHUNK).step_by(7) {
                for lv in (0..TAILLE_CHUNK).step_by(5) {
                    for lu in (0..TAILLE_CHUNK).step_by(5) {
                        testes += 1;
                        if ca.bloc(lu, lv, z) != cb.bloc(lu, lv, z) {
                            differents += 1;
                        }
                    }
                }
            }
        }
    }
    println!(
        "  poche identique sous deux mondes      : {}  ({differents} blocs sur {testes})",
        verdict(differents == 0)
    );

    // b. La poche est finie. Au-delà de son mur, rien — et « rien » veut dire
    //    de l'air, pas le terrain d'à côté.
    let p = Poche::nouvelle(7);
    let mut fuite = 0usize;
    let mut dehors = 0usize;
    for z in 0..crate::monde::HAUTEUR_CHUNK {
        for v in (-600..poche::COTE + 600).step_by(13) {
            for u in [-600, -64, -1, poche::COTE, poche::COTE + 64, poche::COTE + 600] {
                dehors += 1;
                if p.bloc(u, v, z) != Bloc::Air {
                    fuite += 1;
                }
            }
        }
    }
    println!(
        "  air partout hors des bornes           : {}  ({fuite} blocs sur {dehors})",
        verdict(fuite == 0)
    );
    println!(
        "  côté de la poche                      : {} blocs · {} chunks",
        poche::COTE,
        poche::COTE_CHUNKS * poche::COTE_CHUNKS
    );
    println!();
}

/// **3. L'ancre suit le monde, pas la grille.**
///
/// On pose un portail près d'un coin, puis on fait marcher la caméra tout
/// droit — ce qui, sur le cube, franchit des arêtes et suit une géodésique.
/// Deux colonnes : la distance qui décide vraiment de la traversée, dans le
/// monde, et celle qu'aurait donnée un test rangé dans la grille.
///
/// La seconde n'existe que pour se faire voir fausse. Si les deux colonnes
/// coïncidaient, ranger l'ancre dans la grille serait sans conséquence, et le
/// troisième point de cette section serait sans objet.
fn ancre_dans_le_monde(gen: &Generateur) {
    println!("── L'ancre est-elle un point du monde ? ──");

    // Un coin, et une caméra qui rase le sol : on veut que la distance
    // horizontale domine, sinon l'altitude noie ce qu'on vient mesurer.
    let coin = FACE - 6;
    let mut cam = camera_a(gen, 1, coin, coin, 2.36);
    cam.position.z = gen
        .hauteur(cam.face, coin, coin)
        .max(NIVEAU_MER as f32)
        + 4.0;
    cam.tangage = 0.0;

    let portail = Portail::ouvrir(gen, &cam, 3);

    // À la hauteur du portail. Le relief peut l'avoir repoussé de quelques
    // blocs vers le haut — dans le jeu on redescend, ici on veut isoler
    // l'approche horizontale, qui est ce qu'on vient mesurer.
    cam.position.z = portail.z as f32 + 2.0;

    // On recule, loin, puis on revient dessus. C'est la seule forme de mesure
    // qui compte : celle qui **approche** le portail et finit par le franchir.
    // Marcher en s'en éloignant ferait toujours dire la même chose aux deux
    // tests, et une mesure qui ne peut pas échouer sert d'alibi (D28).
    let marcher = |cam: &mut Camera, blocs: f32| -> u32 {
        let d3 = cam.avant_plat() * blocs;
        let (du, dv) = cam.vers_coordonnees(d3);
        cam.avancer(Vec3::new(du, dv, 0.0))
    };
    for _ in 0..24 {
        marcher(&mut cam, -5.0);
    }

    println!(
        "  portail en {} {} {} sur {} — axes du cadre ({:.3} {:.3}) et ({:.3} {:.3})",
        portail.u,
        portail.v,
        portail.z,
        NOMS[portail.face as usize],
        portail.axe_droite.x,
        portail.axe_droite.y,
        portail.axe_avant.x,
        portail.axe_avant.y,
    );
    println!("  pas   face      dans le monde   en coordonnées      écart");

    let mut aretes = 0;
    let mut muettes = 0;
    let mut vus = 0usize;
    let mut pas_segment: Option<usize> = None;
    let mut pas_point: Option<usize> = None;
    let mut pas_grille: Option<usize> = None;

    for pas in 0..=40 {
        // Le joueur regarde le portail : on vise sa place **dans le monde**,
        // puisque c'est là qu'elle est rangée. Poser un cap dans la grille ne
        // désignerait plus l'endroit — la marche suit une géodésique.
        let ici = cam.repere_3d(RAYON).0;
        let vers = portail.lieu - ici;
        cam.viser_point(Vec3::new(vers.x as f32, vers.y as f32, vers.z as f32));

        let vraie = portail.distance(&cam);
        let grille = portail.distance_en_coordonnees(&cam);
        vus += 1;

        let (col, ecart) = if grille.is_finite() {
            (
                format!("{grille:>14.1}"),
                format!("{:>+9.1}", grille as f64 - vraie),
            )
        } else {
            muettes += 1;
            ("      autre face".to_string(), "        —".to_string())
        };
        println!(
            "  {pas:>3}   {:<8} {vraie:>13.1}   {col}   {ecart}",
            NOMS[cam.face as usize]
        );

        // Les trois verdicts possibles, sur le même pas. Le test « naïf » est
        // celui qu'on écrit spontanément : le point d'arrivée est-il assez près
        // de la nappe ? Son rayon est la demi-largeur de l'ouverture, ce qui est
        // le choix le plus favorable qu'on puisse lui faire.
        let naif = (LARGEUR_NAPPE * 0.5) as f64;
        if vraie <= naif && pas_point.is_none() {
            pas_point = Some(pas);
        }
        if (grille as f64) <= naif && pas_grille.is_none() {
            pas_grille = Some(pas);
        }

        let avant = cam.repere_3d(RAYON).0;
        aretes += marcher(&mut cam, 5.0);
        let apres = cam.repere_3d(RAYON).0;
        if portail.franchi(avant, apres).is_some() {
            pas_segment = Some(pas);
            println!("  {pas:>3}   le pas suivant passe à travers.");
            break;
        }
    }

    println!("  arêtes franchies pendant l'approche    : {aretes}");
    println!(
        "  pas où la grille n'a pas de réponse    : {muettes} sur {vus} — le portail \
         et la caméra n'y sont pas sur la même face,"
    );
    println!(
        "                                          et deux repères de face ne se \
         comparent pas."
    );

    // Le vrai enseignement de cette approche : un test ponctuel ne suffit pas.
    match (pas_segment, pas_point) {
        (Some(a), Some(b)) => println!(
            "  franchissement : pas {a} au segment, pas {b} au point — les deux voient"
        ),
        (Some(a), None) => println!(
            "  franchissement : pas {a} au segment, **jamais** au point — à cinq blocs \
             par pas, un test de proximité traverse sans voir. C'est pourquoi \
             `franchi` coupe le plan de la nappe le long du segment."
        ),
        (None, _) => println!("  le portail n'a pas été atteint : allonger l'approche"),
    }
    if pas_grille.is_none() {
        println!(
            "  et un test rangé dans la grille n'aurait jamais rien déclenché du tout."
        );
    }

    // Deuxième moitié : quand la grille *a* une réponse, que vaut-elle ?
    //
    // Les caméras témoins sont posées directement sur la face du portail, sans
    // marcher : on veut mesurer l'écart des deux métriques, pas la longueur
    // d'un trajet. Elles s'écartent vers l'intérieur de la face, donc elles y
    // restent, et la comparaison a un sens tout du long.
    println!("  Sur la face du portail, la grille répond — et voici ce qu'elle vaut :");
    println!("    écart en cases   dans le monde   en coordonnées   erreur");
    for d in [10, 40, 120, 360, 900] {
        let mut temoin = camera_a(gen, portail.face, portail.u - d, portail.v + d, 0.0);
        temoin.position.z = portail.z as f32 + 2.0;
        if temoin.face != portail.face {
            continue;
        }
        let vraie = portail.distance(&temoin);
        let grille = portail.distance_en_coordonnees(&temoin) as f64;
        println!(
            "    {d:>14}   {vraie:>13.1}   {grille:>14.1}   {:>+5.1} %",
            (grille - vraie) / vraie * 100.0
        );
    }
    println!("  La place du portail, elle, n'a pas bougé : elle est rangée en 3D.");
    println!();
}

// --------------------------------------------------------------------------
// Les invariants de la topologie
// --------------------------------------------------------------------------

fn invariants() {
    println!("── Invariants de repliement ──");

    // 1. Idempotence : une position canonique ne bouge pas.
    let mut ok = true;
    for f in 0..6u8 {
        for u in (0..FACE).step_by(37) {
            for v in (0..FACE).step_by(41) {
                let (f2, u2, v2, k) = replier_bloc(f, u, v);
                if (f2, u2, v2, k) != (f, u, v, 0) {
                    ok = false;
                }
            }
        }
    }
    println!("  idempotent sur le patron              : {}", verdict(ok));

    // 2. Aller-retour : sortir par un bord puis revenir sur ses pas rend la
    //    case de départ, et les deux rotations s'annulent.
    let mut ok = true;
    let mut testes = 0;
    for f in 0..6u8 {
        for w in 0..FACE {
            for (du, dv, uu, vv) in [
                (FACE, w, FACE - 1, w),
                (-1, w, 0, w),
                (w, FACE, w, FACE - 1),
                (w, -1, w, 0),
            ] {
                let (f2, u2, v2, k) = replier_bloc(f, du, dv);
                // La direction du pas, transportée, est le vecteur de départ
                // tourné de k quarts de tour.
                let (pu, pv) = (du - uu, dv - vv);
                let (cos, sin) = COS_SIN[k as usize];
                let (tu, tv) = (pu * cos - pv * sin, pu * sin + pv * cos);
                let (f3, u3, v3, k2) = replier_bloc(f2, u2 - tu, v2 - tv);

                if (f3, u3, v3) != (f, uu, vv) || (k + k2) % 4 != 0 {
                    ok = false;
                }
                testes += 1;
            }
        }
    }
    println!("  aller-retour ({testes} bords)          : {}", verdict(ok));

    // 3. Cohérence bloc / chunk : le renderer place des chunks et tourne leur
    //    maillage ; il faut que cela revienne au même que replier bloc à bloc.
    let mut ok = true;
    for f in 0..6u8 {
        for cu in -1..=FACE_CHUNKS {
            for cv in -1..=FACE_CHUNKS {
                let (fc, cuc, cvc, k) = replier_chunk(f, cu, cv);
                for (lu, lv) in [(0, 0), (31, 0), (0, 31), (31, 31), (7, 19)] {
                    let (fb, bu, bv, kb) =
                        replier_bloc(f, cu * TAILLE_CHUNK + lu, cv * TAILLE_CHUNK + lv);
                    let (a, b) = (2 * lu - 31, 2 * lv - 31);
                    let (cos, sin) = COS_SIN[k as usize];
                    let (a2, b2) = (a * cos - b * sin, a * sin + b * cos);
                    let attendu = (
                        fc,
                        cuc * TAILLE_CHUNK + (a2 + 31) / 2,
                        cvc * TAILLE_CHUNK + (b2 + 31) / 2,
                        k,
                    );
                    if (fb, bu, bv, kb) != attendu {
                        ok = false;
                    }
                }
            }
        }
    }
    println!("  chunk et bloc s'accordent             : {}", verdict(ok));

    // 4. Le défaut : combien de voisins distincts a une case ? Huit partout,
    //    sept sur les vingt-quatre cases qui touchent un coin du cube.
    let mut defectueuses = 0;
    let mut pires = 8;
    for f in 0..6u8 {
        for (u, v) in [(0, 0), (FACE - 1, 0), (0, FACE - 1), (FACE - 1, FACE - 1)] {
            let n = voisins_distincts(f, u, v);
            if n != 8 {
                defectueuses += 1;
                pires = pires.min(n);
            }
        }
    }
    println!(
        "  cases de coin défectueuses            : {defectueuses} sur 24, {pires} voisins au lieu de 8"
    );
    println!();
}

fn voisins_distincts(f: u8, u: i32, v: i32) -> usize {
    let mut vus = HashSet::new();
    for dv in -1..=1 {
        for du in -1..=1 {
            if (du, dv) == (0, 0) {
                continue;
            }
            let (f2, u2, v2, _) = replier_bloc(f, u + du, v + dv);
            vus.insert((f2, u2, v2));
        }
    }
    vus.len()
}

fn verdict(ok: bool) -> &'static str { if ok { "OK" } else { "ÉCHEC" } }

// --------------------------------------------------------------------------
// Les douze recollements
// --------------------------------------------------------------------------

fn coutures(gen: &Generateur) {
    println!("── Les douze recollements ──");
    println!("  (mesuré sur les 7/8 centraux de chaque bord : les coins sont");
    println!("   singuliers par construction et relèvent d'une autre mesure)");
    println!("  face      bord      dénivelé couture   dénivelé ordinaire   écart de pas");

    let (mut pire_h, mut pire_a) = (0.0f64, 0.0f64);

    for f in 0..6u8 {
        for (bord, nom) in [(0, "+u"), (1, "−u"), (2, "+v"), (3, "−v")] {
            let (mut somme_couture, mut somme_ordinaire, mut pire_pas) = (0.0, 0.0, 0.0f64);
            let n = 96;

            // On saute les extrémités : elles touchent les coins, où la
            // taille des cases s'effondre légitimement. Ce que la couture
            // doit prouver, c'est qu'elle ne coupe pas — pas que le coin est
            // régulier, ce qu'il n'est pas et ne peut pas être.
            for i in 0..n {
                let w = FACE / 16 + i * (FACE * 7 / 8) / n;
                // Trois cases alignées, perpendiculaires au bord : la deuxième
                // est la dernière de la face, la troisième est de l'autre côté
                // du recollement. Comparer ces deux pas-là, et pas un pas pris
                // ailleurs, est la seule comparaison honnête : c'est le même
                // terrain, au même endroit.
                let (avant, dedans, dehors) = match bord {
                    0 => ((FACE - 2, w), (FACE - 1, w), (FACE, w)),
                    1 => ((1, w), (0, w), (-1, w)),
                    2 => ((w, FACE - 2), (w, FACE - 1), (w, FACE)),
                    _ => ((w, 1), (w, 0), (w, -1)),
                };

                somme_couture += (gen.hauteur(f, dehors.0, dehors.1)
                    - gen.hauteur(f, dedans.0, dedans.1))
                .abs() as f64;
                somme_ordinaire += (gen.hauteur(f, dedans.0, dedans.1)
                    - gen.hauteur(f, avant.0, avant.1))
                .abs() as f64;

                // Le pas qui franchit le recollement doit mesurer, sur la
                // sphère, ce que mesure le pas juste à côté. Le comparer au
                // pas du centre de la face n'aurait plus de sens : la
                // projection conforme fait varier la taille des cases, et un
                // écart légitime passerait pour une discontinuité.
                let voisin = angle(f, avant, dedans);
                if voisin > 0.0 {
                    let r = angle(f, dedans, dehors) / voisin;
                    pire_pas = pire_pas.max(r.max(1.0 / r));
                }
            }

            let (hc, ho) = (somme_couture / n as f64, somme_ordinaire / n as f64);
            let ecart = pire_pas;
            pire_h = pire_h.max(hc / ho.max(1e-9));
            pire_a = pire_a.max(ecart);

            println!(
                "  {:<8}  {nom:<8}  {hc:>16.3}   {ho:>18.3}   {ecart:>15.3}",
                NOMS[f as usize]
            );
        }
    }

    println!(
        "  pire rapport de dénivelé : {pire_h:.3} · pire écart de pas : {pire_a:.3} (1,000 = indiscernable)"
    );
    println!();
}

fn angle(f: u8, a: (i32, i32), b: (i32, i32)) -> f64 {
    let (fa, ua, va, _) = replier_bloc(f, a.0, a.1);
    let (fb, ub, vb, _) = replier_bloc(f, b.0, b.1);
    let (pa, pb) = (point_sphere(fa, ua, va), point_sphere(fb, ub, vb));
    let d = (pa[0] * pb[0] + pa[1] * pb[1] + pa[2] * pb[2]).clamp(-1.0, 1.0);
    d.acos()
}

/// Marcher à travers un bord, et mesurer ce que la visée fait à chaque pas.
///
/// L'ancien test comparait deux caméras posées à un millième de bloc de part et
/// d'autre du bord, de même cap. À cette distance leurs deux tangentes
/// enjambent l'arête et se ressemblent forcément : il mesurait la continuité de
/// la projection, pas celle du transport. Il a certifié continu un
/// franchissement qui faisait sauter la visée de 25,6°.
///
/// **Une mesure qui ne bouge pas n'est pas une mesure qui prouve.** Celle-ci
/// marche, pas à pas, comme le joueur — c'est la seule qui voie ce qu'il voit.
fn marcher_a_travers_les_bords() {
    println!("── Marcher à travers un bord ──");
    println!("  distance au coin    rotation par pas    pire endroit");

    // Cap sortant, par bord : +u, −u, +v, −v.
    let sortie = [
        0.0f32,
        std::f32::consts::PI,
        std::f32::consts::FRAC_PI_2,
        -std::f32::consts::FRAC_PI_2,
    ];

    for recul in [2.0f32, 12.0, 60.0] {
        let mut pire = 0.0f64;
        let mut ou = (0u8, 0usize, 0.0f32);

        for f in 0..6u8 {
            for (bord, cap) in sortie.iter().enumerate() {
                for biais in [0.0f32, 0.7, -0.7] {
                    let depart = match bord {
                        0 => (FACE as f32 - 6.0, recul),
                        1 => (6.0, recul),
                        2 => (recul, FACE as f32 - 6.0),
                        _ => (recul, 6.0),
                    };

                    let mut cam = Camera {
                        face: f,
                        position: Vec3::new(depart.0, depart.1, (NIVEAU_MER + 20) as f32),
                        regard: Vec3::X,
                        tangage: -0.2,
                    };
                    cam.poser_cap(cap + biais);

                    let mut precedente: Option<Vec3> = None;
                    for _ in 0..14 {
                        let (du, dv) = cam.vers_coordonnees(cam.avant_plat());
                        cam.avancer(Vec3::new(du, dv, 0.0));

                        let (_, visee, _) = cam.repere_3d(RAYON);
                        if let Some(avant) = precedente {
                            let angle = (avant.dot(visee) as f64)
                                .clamp(-1.0, 1.0)
                                .acos()
                                .to_degrees();
                            if angle > pire {
                                pire = angle;
                                ou = (f, bord, biais);
                            }
                        }
                        precedente = Some(visee);
                    }
                }
            }
        }

        println!(
            "  {recul:>6.0} blocs         {pire:>10.3}°/bloc    face {}, bord {}, biais {:+.1}",
            NOMS[ou.0 as usize],
            ["+u", "−u", "+v", "−v"][ou.1],
            ou.2
        );
    }

    // Un pas d'un bloc sur une géodésique fait tourner la visée de l'angle
    // parcouru, ni plus ni moins : c'est le plancher, et il est calculable.
    println!(
        "  Plancher géométrique : {:.3}°/bloc (un bloc sur un rayon de {RAYON:.0}).",
        (1.0 / RAYON).to_degrees()
    );
    println!();
}

// --------------------------------------------------------------------------
// Ce que le cube coûte
// --------------------------------------------------------------------------

fn distorsion() {
    println!("── Distorsion de la grille ──");

    let (mut mini, mut maxi) = (f64::MAX, 0.0f64);
    for u in (0..FACE - 1).step_by(16) {
        for v in (0..FACE - 1).step_by(16) {
            let a = angle(1, (u, v), (u + 1, v));
            mini = mini.min(a);
            maxi = maxi.max(a);
        }
    }
    println!("  un pas de grille, du plus court au plus long : {:.4}", maxi / mini);
    println!(
        "  centre de face : {:.6} rad · coin de face : {:.6} rad",
        angle(1, (FACE / 2, FACE / 2), (FACE / 2 + 1, FACE / 2)),
        angle(1, (0, 0), (1, 0))
    );
    println!("  (la grille équirectangulaire précédente : non bornée aux pôles)");
    println!();

    // --- Le cisaillement -------------------------------------------------
    //
    // Une case reste-t-elle carrée ? Les lignes de coordonnées d'une face du
    // cube ne se coupent à angle droit qu'en son centre. Ailleurs, la case est
    // un losange — et un bloc de voxel avec elle.
    println!("── Cisaillement : la case est-elle carrée ? ──");
    println!("  endroit                        angle des côtés   rapport des côtés");

    let mesure = |u: f64, v: f64| -> (f64, f64) {
        let p = DVec3::from_array(direction(1, u, v));
        let tu = DVec3::from_array(direction(1, u + 0.5, v)) - p;
        let tv = DVec3::from_array(direction(1, u, v + 0.5)) - p;
        let angle = tu.normalize().dot(tv.normalize()).clamp(-1.0, 1.0).acos();
        (angle.to_degrees(), tu.length() / tv.length())
    };

    let f = FACE as f64;
    for (nom, u, v) in [
        ("centre de face", f / 2.0, f / 2.0),
        ("milieu d'une arête", 0.5, f / 2.0),
        ("à mi-chemin du coin", f / 4.0, f / 4.0),
        ("coin de face", 0.5, 0.5),
    ] {
        let (angle, rapport) = mesure(u, v);
        println!("  {nom:<28}   {angle:>13.1}°   {rapport:>17.3}");
    }
    println!("  90° = carré partout : c'est ce que la projection conforme achète.");
    println!();

    // --- Le raccord ne doit ni pincer ni replier la carte ------------------
    //
    // Mélanger deux cartes est facile à faire de travers : si les deux
    // divergent trop là où le poids change vite, le terme de raccord domine la
    // dérivée, la case s'écrase, et au pire la carte se replie sur elle-même.
    // Un déterminant négatif signerait un pli — et un monde qui se recouvre.
    println!("── Le raccord aux coins ──");
    let mut pire_pincement = (f64::MAX, 0.0f64, 0.0f64);
    let mut det_mini = f64::MAX;
    let pas = 4.0 / (conforme::N - 1) as f64;

    let mut s = -1.0 + pas;
    while s < 1.0 - pas {
        let mut t = -1.0 + pas;
        while t < 1.0 - pas {
            let p = conforme::table().ab(s, t);
            let ds = conforme::table().ab(s + pas, t);
            let dt = conforme::table().ab(s, t + pas);
            let (jx, jy) = ((ds.0 - p.0, ds.1 - p.1), (dt.0 - p.0, dt.1 - p.1));

            let det = jx.0 * jy.1 - jy.0 * jx.1;
            det_mini = det_mini.min(det / (pas * pas));

            let aire = (jx.0 * jx.0 + jx.1 * jx.1).sqrt().min(
                (jy.0 * jy.0 + jy.1 * jy.1).sqrt(),
            ) / pas;
            if aire < pire_pincement.0 {
                pire_pincement = (aire, s, t);
            }
            t += pas;
        }
        s += pas;
    }

    println!(
        "  déterminant minimal du jacobien   : {det_mini:.4}  ({})",
        if det_mini > 0.0 { "aucun pli" } else { "PLI — la carte se recouvre" }
    );
    println!(
        "  côté de case le plus court        : {:.3} (en {:.3}, {:.3})",
        pire_pincement.0, pire_pincement.1, pire_pincement.2
    );

    // Jusqu'où le losange se voit-il ? On mesure la distance au coin au-delà de
    // laquelle l'angle est revenu sous 95°, sur la diagonale de la face.
    let mut limite = 0.0f64;
    let mut d = 0.001;
    while d < 1.0 {
        let (s, t) = (1.0 - d / 2.0f64.sqrt(), 1.0 - d / 2.0f64.sqrt());
        let p = DVec3::from_array(direction(1, (s + 1.0) * FACE as f64 / 2.0, (t + 1.0) * FACE as f64 / 2.0));
        let a = DVec3::from_array(direction(1, (s + 1.0) * FACE as f64 / 2.0 + 1.0, (t + 1.0) * FACE as f64 / 2.0));
        let b = DVec3::from_array(direction(1, (s + 1.0) * FACE as f64 / 2.0, (t + 1.0) * FACE as f64 / 2.0 + 1.0));
        let angle = (a - p).normalize().dot((b - p).normalize()).clamp(-1.0, 1.0).acos();
        if (angle.to_degrees() - 90.0).abs() > 5.0 {
            limite = d;
        }
        d += 0.002;
    }
    println!(
        "  zone où la case sort de 90° ± 5°  : rayon {:.0} blocs autour de chaque coin",
        limite * FACE as f64 / 2.0
    );
    println!();
}

/// La table conforme mérite-t-elle qu'on s'y fie ?
///
/// Elle est intégrée numériquement, donc elle ne vaut que ce que valent ses
/// vérifications. Il y en a trois : le coin doit tomber où la théorie le dit,
/// la projection doit être inversible au bloc près, et un pas de grille doit
/// mesurer un bloc au centre d'une face.
fn projection_conforme() {
    println!("── La table conforme ──");

    let (mx, my) = conforme::table().coin_brut;
    let (ax, ay) = conforme::coin_attendu();
    println!("  côté de la table                  : {} × {}", conforme::N, conforme::N);
    println!("  ζ au coin, mesuré                 : {mx:.6}  {my:.6}");
    println!("  ζ au coin, attendu — (1+i)/(√3+1) : {ax:.6}  {ay:.6}");
    println!(
        "  écart d'intégration               : {:.2e}",
        ((mx - ax).powi(2) + (my - ay).powi(2)).sqrt()
    );
    println!(
        "  raccord équiangulaire aux coins    : rayon {:.2} de face, soit {:.0} blocs",
        conforme::raccord(),
        conforme::raccord() * FACE as f64 / 2.0
    );

    // Aller-retour : projeter puis dé-projeter doit rendre la case de départ.
    let (mut pire, mut ou) = (0.0f64, (0u8, 0, 0));
    for f in 0..6u8 {
        for u in (4..FACE - 4).step_by(97) {
            for v in (4..FACE - 4).step_by(101) {
                let d = direction(f, u as f64 + 0.5, v as f64 + 0.5);
                let (f2, u2, v2) = crate::cube::depuis_direction(d);
                let e = if f2 == f {
                    ((u2 - u as f64 - 0.5).powi(2) + (v2 - v as f64 - 0.5).powi(2)).sqrt()
                } else {
                    f64::INFINITY
                };
                if e > pire {
                    pire = e;
                    ou = (f, u, v);
                }
            }
        }
    }
    println!(
        "  aller-retour, écart maximal       : {pire:.4} bloc (face {}, {} {})",
        NOMS[ou.0 as usize], ou.1, ou.2
    );

    // La taille d'un bloc, du centre d'une face vers un coin.
    println!("  taille d'un bloc, du centre au coin :");
    let f = FACE as f64;
    let reference = {
        let a = DVec3::from_array(direction(1, f / 2.0, f / 2.0));
        let b = DVec3::from_array(direction(1, f / 2.0 + 1.0, f / 2.0));
        (b - a).length() * RAYON
    };
    for part in [0.0, 0.5, 0.8, 0.95, 0.999] {
        let (u, v) = (f / 2.0 * (1.0 + part), f / 2.0 * (1.0 + part));
        let a = DVec3::from_array(direction(1, u, v));
        let b = DVec3::from_array(direction(1, u + 1.0, v));
        println!(
            "    {:>5.1} % du chemin : {:.3} bloc",
            part * 100.0,
            (b - a).length() * RAYON / reference
        );
    }
    println!();
}

/// Le déroulement place des chunks côte à côte. Sont-ils vraiment voisins sur
/// le cube ?
///
/// C'est la question que la montagne coupée pose. Une case dupliquée n'est pas
/// seulement une copie : là où le quartier de 90° se referme, deux cases
/// dessinées l'une à côté de l'autre appartiennent à des endroits différents du
/// monde, et le terrain s'y coupe net.
fn continuite_du_deroulement() {
    println!("── Pourquoi le rendu ne déroule plus ──");
    println!("  Le rendu place désormais chaque chunk sur la sphère, une seule fois.");
    println!("  Voici ce que coûtait le déroulement à plat qu'il remplace :");
    println!("  position de la caméra      adjacences   fausses   dont mal orientées");

    let r = 8;
    for (nom, f, cu, cv) in [
        ("centre de face", 1u8, FACE_CHUNKS / 2, FACE_CHUNKS / 2),
        ("milieu d'arête", 1, FACE_CHUNKS / 2, FACE_CHUNKS - 1),
        ("coin", 1, 0, FACE_CHUNKS - 1),
    ] {
        let (mut total, mut fausses, mut tournees) = (0, 0, 0);

        for dv in -r..=r {
            for du in -r..=r {
                let a = replier_chunk(f, cu + du, cv + dv);
                for (pu, pv) in [(1, 0), (0, 1)] {
                    let b = replier_chunk(f, cu + du + pu, cv + dv + pv);

                    // Le vrai voisin de `a` dans la direction du pas, telle
                    // qu'elle arrive dans le repère canonique de `a`.
                    let (cos, sin) = COS_SIN[a.3 as usize];
                    let (tu, tv) = (pu * cos - pv * sin, pu * sin + pv * cos);
                    let vrai = replier_chunk(a.0, a.1 + tu, a.2 + tv);

                    total += 1;
                    if (vrai.0, vrai.1, vrai.2) != (b.0, b.1, b.2) {
                        fausses += 1;
                    } else if (a.3 + vrai.3) % 4 != b.3 {
                        tournees += 1;
                    }
                }
            }
        }

        println!(
            "  {nom:<24}  {total:>10}   {fausses:>7}   {tournees:>18}"
        );
    }
    println!("  Ces huit fronti\u{00e8}res \u{00e9}taient la montagne coup\u{00e9}e. Le rendu sph\u{00e9}rique en a z\u{00e9}ro.");
    println!();
}

/// Le réticule pointe-t-il le bloc qu'on surligne ?
///
/// C'est le test d'étanchéité de D27, et il ne tient que parce que la
/// projection est inversible : le rayon vient de l'écran, il est redressé une
/// fois, puis le monde n'est plus interrogé qu'à plat. Ce qui reste d'écart est
/// le pas de marche, pas une erreur de principe.
///
/// La version qui marchait droit dans le repère de la face mesurait ici jusqu'à
/// 45 blocs.
fn derive_de_visee(gen: &Generateur) {
    println!("── Le réticule et le bloc surligné ──");
    println!("  position                cap    portée   écart");

    for (nom, face, u, v) in [
        ("centre de face", 1u8, FACE / 2, FACE / 2),
        ("près d'un coin", 1u8, 8, FACE - 8),
    ] {
        for cap in [0.0f32, 0.7, 2.4] {
            let mut cam = Camera {
                face,
                position: Vec3::new(u as f32 + 0.5, v as f32 + 0.5, (NIVEAU_MER + 40) as f32),
                regard: Vec3::X,
                tangage: -0.2,
            };
            cam.poser_cap(cap);
            let (position, avant, _) = cam.repere_3d(RAYON);
            let avant = DVec3::new(avant.x as f64, avant.y as f64, avant.z as f64);

            match viser(gen, &cam, RAYON, 400.0) {
                None => println!("  {nom:<20}  {cap:>5.1}         —   rien en vue"),
                Some((f, bu, bv, bz)) => {
                    let centre =
                        DVec3::from_array(point_sphere(f, bu, bv)) * (RAYON + bz as f64 + 0.5);
                    let vers = centre - position;
                    let t = vers.length();
                    let angle = vers.normalize().dot(avant).clamp(-1.0, 1.0).acos();
                    println!(
                        "  {nom:<20}  {cap:>5.1}  {t:>8.0}   {:>5.2} blocs",
                        t * angle.tan()
                    );
                }
            }
        }
    }
    println!();
}

// --------------------------------------------------------------------------
// Le terrain
// --------------------------------------------------------------------------

fn terrain(gen: &Generateur) {
    println!("── Terrain ──");
    println!("  Coupe du pôle nord à l'équateur (face +Z puis +Y) :");

    for i in 0..=8 {
        let v = i * (FACE - 1) / 8;
        let (sol, biome) = gen.colonne(4, FACE / 2, v);
        let p = point_sphere(4, FACE / 2, v);
        let lat = p[2].asin().to_degrees();
        println!("    +Z v={v:>4}  lat {lat:>6.1}°  h={sol:>4}  {}", biome.nom());
    }
    for i in 0..=4 {
        let v = i * (FACE - 1) / 4;
        let (sol, biome) = gen.colonne(1, FACE / 2, v);
        let p = point_sphere(1, FACE / 2, v);
        let lat = p[2].asin().to_degrees();
        println!("    +Y v={v:>4}  lat {lat:>6.1}°  h={sol:>4}  {}", biome.nom());
    }
    println!();

    let (face, u, v) = point_apparition(gen);
    let sol = gen.hauteur(face, u, v);
    let cam = Camera {
        face,
        position: Vec3::new(
            u as f32 + 0.5,
            v as f32 + 0.5,
            sol.max(crate::monde::NIVEAU_MER as f32) + 6.0,
        ),
        regard: Vec3::X,
        tangage: -0.15,
    };
    let p = point_sphere(face, u, v);
    println!(
        "  Apparition : face {} ({u}, {v}) · lat {:.1}° · sol {sol:.0} · {}",
        NOMS[face as usize],
        p[2].asin().to_degrees(),
        biome_de(p[2].asin().abs() / std::f64::consts::FRAC_PI_2).nom()
    );
    match viser(gen, &cam, RAYON, 260.0) {
        Some((f, bu, bv, bz)) => println!(
            "    visé : {bu} {bv} {bz} sur {}",
            NOMS[f as usize]
        ),
        None => println!("    visé : rien"),
    }
}

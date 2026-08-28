//! Le film de l'ancre : `proto-sphere --film-ancre <dossier>`.
//!
//! L'aller-retour complet, image par image : le monde ordinaire, le portail qui
//! s'ouvre, la traversée, la salle du passé, le demi-tour, le retour **par le
//! portail**, et la fenêtre qui se referme.
//!
//! Le film **conduit le jeu**, il ne le double pas. Chaque image applique un
//! ordre par les mêmes méthodes que le clavier — `App::deplacer`,
//! `App::pivoter`, `App::actionner_ancre`. Un film qui referait le déplacement
//! de son côté filmerait sa propre copie du programme, et ne dirait rien de
//! celui-ci.
//!
//! **Ce qu'il mesure : que la fenêtre ne mentait pas.**
//!
//! « Sans coupure » est facile à dire et difficile à prouver, et les deux
//! nombres qui viennent d'abord à l'esprit ne valent rien.
//!
//! La distance entre la position d'avant et celle d'après vaut zéro **par
//! construction** : c'est la même transformation qui peint la fenêtre et qui
//! fait passer le joueur, donc elle ne prouverait que sa propre définition.
//!
//! Et l'écart entre deux images consécutives est grand au franchissement, comme
//! il doit l'être : le cadre balaie l'écran quand on le passe, exactement comme
//! le chambranle d'une porte. Une mesure plein cadre ne sait pas distinguer une
//! porte franchie d'une coupure. Elle est de la mauvaise forme, et le premier
//! essai de ce film l'a montré — 8,7 fois l'écart des pas voisins, pour une
//! traversée pourtant continue.
//!
//! Ce qu'on compare est donc **ce que la nappe montrait à l'image d'avant** —
//! les coulisses, redescendues du GPU — et **ce qu'on a obtenu en la
//! franchissant**. Deux rendus pleine page du même monde, depuis deux caméras
//! qui ne diffèrent que du reste du pas. Si la transformation était fausse, ils
//! ne se ressembleraient pas. On le compare enfin à l'écart de deux images
//! consécutives ordinaires du même acte : c'est la forme de mesure du film de
//! coin — on ne demande pas si le chiffre est petit, on demande s'il se
//! distingue de ses voisins.

use crate::App;
use crate::ancre::Sejour;

/// Ce qu'une image demande au jeu de faire.
#[derive(Clone, Copy)]
pub enum Ordre {
    /// Avancer de tant de blocs.
    Marcher(f32),
    /// Tourner le regard de tant de radians.
    Tourner(f32),
    /// Ne rien faire : laisser voir.
    Attendre,
    /// Presser `P`.
    Levier,
}

/// Le découpage du film. Sert la piste de la page, et rien d'autre.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Acte {
    Approche,
    Ouverture,
    Aller,
    Poche,
    DemiTour,
    Retour,
    Fermeture,
}

impl Acte {
    pub fn nom(self) -> &'static str {
        match self {
            Acte::Approche => "le présent",
            Acte::Ouverture => "l'ancre s'ouvre",
            Acte::Aller => "vers le passé",
            Acte::Poche => "le passé",
            Acte::DemiTour => "demi-tour",
            Acte::Retour => "vers le présent",
            Acte::Fermeture => "la fenêtre se referme",
        }
    }
}

pub struct Etape {
    pub acte: Acte,
    pub monde: &'static str,
    pub face: String,
    pub u: i32,
    pub v: i32,
    pub z: i32,
    /// Distance au portail, dans le monde. Absente une fois de l'autre côté.
    pub distance: Option<f64>,
    /// Ce qu'il reste de la fenêtre, en secondes.
    pub reste: Option<f32>,
    /// Écart moyen avec l'image précédente, en niveaux sur 255.
    pub saut: f32,
    pub evenement: Option<&'static str>,
}

pub struct FilmAncre {
    pub image: usize,
    pub dossier: String,
    pub journal: Vec<Etape>,
    plan: Vec<(Acte, Ordre)>,
    /// L'image précédente, en RGBA brut : de quoi mesurer le saut.
    precedente: Option<Vec<u8>>,
    saut: f32,
    /// Ce que la fenêtre montrait à l'image d'avant : les coulisses, relues.
    apercu: Option<Vec<u8>>,
    /// Le verdict de chaque franchissement : (image, quoi, écart à l'aperçu,
    /// écart moyen des pas voisins).
    pub franchissements: Vec<(usize, &'static str, f32, f32)>,
    attente: Option<&'static str>,
}

/// Le scénario, un ordre par image.
///
/// Les longueurs ne sont pas décoratives. Il faut assez d'images de part et
/// d'autre de chaque franchissement pour que la mesure ait des **voisins** à
/// qui se comparer : un saut ne se juge que contre les pas ordinaires du même
/// acte, à la même cadence.
fn scenario() -> Vec<(Acte, Ordre)> {
    let mut p: Vec<(Acte, Ordre)> = Vec::new();
    let mut pousser = |acte, ordre, n| {
        for _ in 0..n {
            p.push((acte, ordre));
        }
    };

    // Le monde ordinaire : on marche, il ne se passe rien.
    pousser(Acte::Approche, Ordre::Marcher(2.2), 16);
    pousser(Acte::Approche, Ordre::Attendre, 4);

    // L'ancre s'ouvre, et on la regarde. La nappe montre déjà le passé.
    pousser(Acte::Ouverture, Ordre::Levier, 1);
    pousser(Acte::Ouverture, Ordre::Attendre, 12);

    // On s'en approche à pas lents et réguliers — c'est la cadence qui rend le
    // franchissement comparable à ses voisins — et on passe au travers.
    pousser(Acte::Aller, Ordre::Marcher(0.42), 26);

    // Le passé. On s'éloigne du portail en ligne droite — puis on regarde à
    // droite et on revient au même cap. Le tour est exactement compensé : sans
    // cela, le demi-tour ne ramènerait pas sur ses propres traces, et le
    // chemin du retour manquerait l'ouverture.
    pousser(Acte::Poche, Ordre::Marcher(1.5), 24);
    pousser(Acte::Poche, Ordre::Tourner(0.07), 12);
    pousser(Acte::Poche, Ordre::Tourner(-0.07), 12);
    pousser(Acte::Poche, Ordre::Attendre, 4);

    // Demi-tour, à un demi-tour près : 24 × 0,1309 = π.
    pousser(Acte::DemiTour, Ordre::Tourner(0.1309), 24);
    pousser(Acte::DemiTour, Ordre::Attendre, 10);

    // On revient par où l'on est venu, et à la même cadence lente près du
    // seuil qu'à l'aller : c'est ce qui rend les deux franchissements
    // comparables l'un à l'autre.
    pousser(Acte::Retour, Ordre::Marcher(1.4), 20);
    pousser(Acte::Retour, Ordre::Marcher(0.42), 30);
    pousser(Acte::Retour, Ordre::Attendre, 6);

    // Et la fenêtre se referme derrière soi.
    pousser(Acte::Fermeture, Ordre::Levier, 1);
    pousser(Acte::Fermeture, Ordre::Attendre, 12);

    p
}

impl FilmAncre {
    pub fn nouveau(dossier: String) -> Self {
        std::fs::create_dir_all(&dossier).expect("dossier du film");
        Self {
            image: 0,
            dossier,
            journal: Vec::new(),
            plan: scenario(),
            precedente: None,
            saut: 0.0,
            apercu: None,
            franchissements: Vec::new(),
            attente: None,
        }
    }

    pub fn images(&self) -> usize {
        self.plan.len()
    }

    /// L'ordre de l'image courante, ou `None` quand le film est fini.
    pub fn ordre(&self) -> Option<(Acte, Ordre)> {
        self.plan.get(self.image).copied()
    }

    /// L'écart moyen entre deux images, en niveaux sur 255.
    fn ecart(a: &[u8], b: &[u8]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }
        let somme: u64 = a
            .as_chunks::<4>()
            .0
            .iter()
            .zip(b.as_chunks::<4>().0.iter())
            .map(|(x, y)| {
                (x[0].abs_diff(y[0]) as u64)
                    + (x[1].abs_diff(y[1]) as u64)
                    + (x[2].abs_diff(y[2]) as u64)
            })
            .sum();
        somme as f32 / (a.len() as f32 / 4.0 * 3.0)
    }

    /// L'image vient d'arriver. On mesure, et on retient ce qu'il faut.
    ///
    /// **La mesure du franchissement n'est pas l'écart avec l'image d'avant.**
    /// Celui-là est grand, et il doit l'être : le cadre du portail balaie
    /// l'écran quand on le passe, exactement comme le chambranle d'une porte.
    /// Un écart plein cadre ne sait pas distinguer une porte franchie d'une
    /// coupure — c'est une mesure de la mauvaise forme.
    ///
    /// Ce qu'on compare, c'est **ce que la fenêtre montrait à l'image d'avant**
    /// — les coulisses, relues du GPU — et **ce qu'on a obtenu en la
    /// franchissant**. Les deux sont des rendus pleine page du même monde,
    /// depuis deux caméras qui ne diffèrent que du reste du pas. Si la
    /// transformation était fausse, ils ne se ressembleraient pas.
    pub fn mesurer(&mut self, rgba: &[u8], apercu: Option<Vec<u8>>) {
        self.saut = match &self.precedente {
            None => 0.0,
            Some(avant) => Self::ecart(avant, rgba),
        };
        if let Some(quoi) = self.attente.take() {
            if let Some(vu) = &self.apercu {
                let ecart = Self::ecart(vu, rgba);
                self.franchissements.push((self.image + 1, quoi, ecart, 0.0));
            }
        }
        self.precedente = Some(rgba.to_vec());
        self.apercu = apercu;
    }

    /// Cette image a franchi : la prochaine mesure comparera à l'aperçu.
    pub fn signaler_traversee(&mut self, quoi: &'static str) {
        self.attente = Some(quoi);
    }

    /// Note ce que l'image montre, une fois l'ordre appliqué.
    pub fn noter(&mut self, app: &App, evenement: Option<&'static str>) {
        let acte = self.acte_courant();
        let etape = match &app.sejour {
            Sejour::Sphere => {
                let (f, u, v, _) = crate::cube::replier_bloc(
                    app.cam.face,
                    app.cam.position.x.floor() as i32,
                    app.cam.position.y.floor() as i32,
                );
                Etape {
                    acte,
                    monde: "présent",
                    face: crate::cube::NOMS[f as usize].to_string(),
                    u,
                    v,
                    z: app.cam.position.z as i32,
                    distance: app.portail.as_ref().map(|p| p.distance(&app.cam)),
                    reste: app.fenetre,
                    saut: self.saut,
                    evenement,
                }
            }
            Sejour::Poche { cam, .. } => Etape {
                acte,
                monde: "passé",
                face: "poche".to_string(),
                u: cam.position.x as i32,
                v: cam.position.y as i32,
                z: cam.position.z as i32,
                distance: None,
                reste: app.fenetre,
                saut: self.saut,
                evenement,
            },
        };
        self.journal.push(etape);
        self.image += 1;
    }

    fn acte_courant(&self) -> Acte {
        self.plan.get(self.image).map(|(a, _)| *a).unwrap_or(Acte::Fermeture)
    }

    /// Enregistre l'image courante.
    pub fn poser(&self, octets: &[u8]) {
        let chemin = format!("{}/image{:03}.jpg", self.dossier, self.image);
        let fichier = std::fs::File::create(&chemin).expect("image du film");
        let mut sortie = std::io::BufWriter::new(fichier);

        let rgb: Vec<u8> = octets
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();

        let mut codeur =
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut sortie, 74);
        codeur
            .encode(
                &rgb,
                crate::film::LARGEUR,
                crate::film::HAUTEUR,
                image::ExtendedColorType::Rgb8,
            )
            .expect("encodage JPEG");
    }

    /// Le verdict, une fois le film fini : à chaque franchissement, l'écart
    /// entre ce que la fenêtre montrait et ce qu'on a obtenu, et l'écart moyen
    /// de deux images consécutives du même acte, pour comparer.
    ///
    /// On ne compare qu'à l'intérieur du même acte : un pas de 1,4 bloc bouge
    /// plus l'image qu'un pas de 0,42, et mélanger les deux ferait dire
    /// n'importe quoi à la moyenne.
    fn verdict(&self) -> Vec<(usize, &'static str, f32, f32)> {
        self.franchissements
            .iter()
            .map(|(image, quoi, ecart, _)| {
                let acte = self.journal.get(image - 1).map(|e| e.acte);
                let voisins: Vec<f32> = self
                    .journal
                    .iter()
                    .enumerate()
                    .filter(|(j, v)| {
                        Some(v.acte) == acte && *j + 1 != *image && v.saut > 0.0
                    })
                    .map(|(_, v)| v.saut)
                    .collect();
                let moyenne = if voisins.is_empty() {
                    0.0
                } else {
                    voisins.iter().sum::<f32>() / voisins.len() as f32
                };
                (*image, *quoi, *ecart, moyenne)
            })
            .collect()
    }

    /// Le journal, en JSON, pour la page qui montre le film.
    pub fn ecrire_journal(&self) {
        use std::io::Write;
        let chemin = format!("{}/trajet.json", self.dossier);
        let mut f = std::fs::File::create(chemin).expect("journal du film");
        writeln!(f, "[").unwrap();
        for (i, e) in self.journal.iter().enumerate() {
            let virgule = if i + 1 == self.journal.len() { "" } else { "," };
            let opt = |v: Option<f64>| match v {
                Some(x) => format!("{x:.2}"),
                None => "null".into(),
            };
            writeln!(
                f,
                r#"  {{"acte":"{}","monde":"{}","face":"{}","u":{},"v":{},"z":{},"distance":{},"reste":{},"saut":{:.3},"evenement":{}}}{}"#,
                e.acte.nom(),
                e.monde,
                e.face,
                e.u,
                e.v,
                e.z,
                opt(e.distance),
                opt(e.reste.map(|x| x as f64)),
                e.saut,
                match e.evenement {
                    Some(x) => format!("\"{x}\""),
                    None => "null".into(),
                },
                virgule
            )
            .unwrap();
        }
        writeln!(f, "]").unwrap();

        let chemin = format!("{}/verdict.json", self.dossier);
        let mut f = std::fs::File::create(chemin).expect("verdict du film");
        let v = self.verdict();
        writeln!(f, "{{\"images\":{},\"franchissements\":[", self.journal.len()).unwrap();
        for (i, (image, nom, saut, voisins)) in v.iter().enumerate() {
            let virgule = if i + 1 == v.len() { "" } else { "," };
            writeln!(
                f,
                r#"  {{"image":{image},"quoi":"{nom}","saut":{saut:.3},"voisins":{voisins:.3}}}{virgule}"#
            )
            .unwrap();
        }
        writeln!(f, "]}}").unwrap();
    }

    /// Ce qu'on imprime à la fin, en console.
    pub fn resumer(&self) {
        println!("── La fenêtre disait-elle vrai ? ──");
        println!("  image   franchissement                    fenêtre → obtenu   un pas ordinaire");
        for (image, nom, ecart, voisins) in self.verdict() {
            println!("  {image:>5}   {nom:<32} {ecart:>16.2}   {voisins:>16.2}");
        }
        println!("  (écart moyen par pixel, en niveaux sur 255)");
        println!("  À gauche : ce que la nappe montrait à l'image d'avant, contre ce qu'on a");
        println!("  obtenu en la franchissant. À droite : deux images consécutives du même");
        println!("  acte. Si la première n'excède pas la seconde, la traversée n'a pas de");
        println!("  couture — elle coûte exactement un pas.");
    }
}

//! Les réglages du climat et des biomes, en données plutôt qu'en constantes.
//!
//! Une vingtaine de nombres décident de la carte : où passent les bandes de
//! D24, à quelle hauteur la neige, quelle part du monde revient à chaque
//! région de D19. Tant qu'ils étaient des `const`, en voir l'effet demandait
//! de recompiler *et* de régénérer un monde — une boucle de plusieurs minutes
//! pour un chiffre qu'on voulait bouger de deux centièmes. Q27 les liste comme
//! ouverts depuis D39 et ils le sont restés pour cette seule raison.
//!
//! Les voici groupés et nommés. Le défaut est ce que le jeu emploie ;
//! `world/examples/reglages.rs` en fait varier les valeurs devant des
//! curseurs, sans jamais réécrire la loi — elle reste dans
//! [`super::SimChunk::generate`] et [`super::SimChunk::get_biome`], en un seul
//! exemplaire.

use serde::{Deserialize, Serialize};

/// Comment la latitude, l'altitude et le bruit composent la température.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bandes {
    /// Les trois poids de `cdf_irwin_hall` : latitude, altitude, bruit.
    ///
    /// **La latitude domine, et c'est tout le propos de D39.** La version
    /// d'amont donnait le poids double à l'altitude, si bien que le monde
    /// était chaud là où il était bas et que les bandes de D24 n'existaient
    /// nulle part.
    pub poids: [f32; 3],

    /// De combien le bruit fait onduler la frontière d'une bande.
    ///
    /// Sur l'échelle de `sin(latitude)`, donc en part de demi-hémisphère. À
    /// zéro, les bandes sont des parallèles tracés à la règle ; trop haut,
    /// elles cessent de se lire.
    pub onde: f64,
}

/// Ce qui fait descendre la température quand on monte.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Etagement {
    /// Unités de température perdues par bloc au-dessus du niveau de la mer.
    ///
    /// **C'est lui, et lui seul, qui met la neige au sommet d'une montagne
    /// équatoriale.** Il ne passe pas par le quantile : l'altitude y entre
    /// déjà comme un *rang* sur la carte, et « être dans les dix pour cent les
    /// plus hauts » ne veut pas dire « être à 1 500 blocs ».
    pub gradient: f32,

    /// De combien un volcan réchauffe son voisinage.
    ///
    /// Trois effets pour un terme : la neige ne tient pas sur le cône, le test
    /// `Volcanic` passe avant tout test de température, et la transition
    /// depuis une chaîne enneigée est douce au lieu d'être une ligne franche.
    pub halo: f32,
}

/// De combien la chaleur assèche.
///
/// **Le plancher est ce qui compte.** Sans lui le facteur atteint zéro, et
/// aucune humidité brute — qui ne dépasse jamais 1 — ne peut alors satisfaire
/// une jungle. C'est ce qui l'avait ramenée à 0,2 % du monde et fait du désert
/// la deuxième terre : au-dessus de `temp = 0,8`, l'humidité passait sous 0,15
/// presque partout, ce qui *est* la condition du désert.
///
/// Le terme d'amont était réglé pour un monde où la température venait du
/// bruit et ne saturait jamais. Branché derrière la latitude, il sature tout
/// le temps.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Evaporation {
    /// Température à partir de laquelle l'assèchement commence.
    pub seuil: f32,
    /// Sur combien d'unités de température il atteint son plancher.
    pub pente: f32,
    /// Part de l'humidité qu'il ne prendra jamais.
    pub plancher: f32,
}

/// Les deux calottes et l'abysse.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Calottes {
    /// Où commence la banquise, en `sin(latitude)`.
    pub banquise: f32,

    /// Où commence la barrière de glace, en `sin(latitude)`.
    ///
    /// **Il ne peut pas descendre sous 0,707.** Une face polaire couvre `|z|`
    /// de 0,577 à ses coins et 0,707 au milieu de ses arêtes : en dessous,
    /// l'anneau du front de falaise enjambe les quatre coutures de la face.
    pub barriere: f32,

    /// À partir de quelle hauteur d'eau l'océan devient l'abysse, en blocs.
    ///
    /// Le fond ne descend pas si bas qu'on le croit : le niveau de la mer est
    /// à 140 et le point le plus bas du monde vers 14. Un seuil à 250
    /// n'ouvrait sur rien — et un seuil qui n'ouvre sur rien est une panne
    /// muette, pas un réglage.
    pub abysse: f32,
}

/// Le plus petit `|sin(latitude)|` qui tienne dans une face polaire.
///
/// Une face polaire couvre `|z|` de 0,577 à ses coins et **0,707 au milieu de
/// ses arêtes**. Tout anneau tracé sous cette valeur enjambe les quatre
/// coutures de la face — c'est la contrainte de D42, et elle vaut aussi bien
/// pour le seuil d'une calotte que pour l'ondulation de son front.
pub const PLANCHER_FACE: f32 = 0.707;

/// Le relief des deux calottes : ce qui les distingue d'une table.
///
/// Rien de tout cela ne touche à l'altitude de simulation. Les calottes
/// restent un traitement de surface posé sur une colonne océanique (D42) :
/// ni le soulèvement ni l'érosion ne bougent, et la carte du monde continue
/// de les montrer lisses.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Relief {
    /// Largeur d'une plaque de banquise, en blocs.
    pub floe_taille: f64,

    /// De combien deux plaques voisines se décalent, en blocs.
    ///
    /// C'est la lecture `Value` du Worley : une constante par cellule, donc
    /// un niveau propre à chaque plaque. Sans elle, les plaques ne se
    /// distinguent que par leurs bords et la banquise redevient une table
    /// gravée.
    pub floe_devers: f32,

    /// Où commence la jointure entre deux plaques, sur la sortie brute du
    /// Worley en `Distance`.
    ///
    /// **Ces deux seuils se lisent dans l'échelle du bruit, pas en blocs**, et
    /// cette échelle n'est pas la même à plat que sur un cube : la lecture y
    /// est 2D d'un côté, 3D de l'autre. `cube_monde --calottes` imprime la
    /// distribution observée — c'est elle qui les règle, pas le jugement.
    pub crete_seuil: f32,

    /// Hauteur d'une crête de compression, en blocs.
    ///
    /// Deux plaques qui se poussent bourrelettent au contact. La crête retombe
    /// au cœur de la jointure, là où la crevasse la fend.
    pub crete_hauteur: f32,

    /// Où la jointure se fend, sur la même échelle que `crete_seuil`.
    pub crevasse_seuil: f32,

    /// Sur combien la fente atteint sa pleine profondeur, même échelle.
    pub crevasse_largeur: f32,

    /// Profondeur d'une crevasse au bord de la calotte, en blocs.
    ///
    /// Au bord seulement : elle est multipliée par l'ouverture, qui se referme
    /// en montant vers le pôle.
    pub crevasse_profondeur: f32,

    /// Ce qui doit rester de glace sous une crevasse, en blocs.
    ///
    /// **C'est lui qui la garde sèche.** Une crevasse qui perce la dalle
    /// ouvre sur l'océan, et ce n'est plus une crevasse : c'est un chenal.
    pub crevasse_plancher: f32,

    /// De combien la banquise se referme au pôle, entre 0 et 1.
    ///
    /// « Une couverture qui se referme à mesure qu'on monte » (D42) : les
    /// crevasses sont pleines au seuil et se resserrent en montant. **Se
    /// refermer n'est pas disparaître** — à 1, la moitié de la calotte
    /// redevient une table, et c'est ce qui est arrivé au premier jet.
    pub crevasse_fermeture: f32,

    /// Ce que la banquise porte au-dessus de l'eau, et ce qu'elle y plonge.
    ///
    /// Leur somme est son épaisseur, et elle doit loger une crevasse : sous
    /// une quinzaine de blocs, la fente n'a pas la place d'exister.
    pub banquise_francbord: f32,
    pub banquise_tirant: f32,

    /// Les mêmes pour la barrière du sud. **Le franc-bord *est* la hauteur de
    /// la falaise du front**, et le tirant ce sous quoi passe le sous-marin.
    pub barriere_francbord: f32,
    pub barriere_tirant: f32,

    /// De combien la surface de la barrière respire, en blocs.
    ///
    /// « Falaise sur les bords puis pratiquement plat » est la définition d'un
    /// front de barrière : ce nombre reste petit, sans quoi le sud cesse de se
    /// distinguer du nord.
    pub barriere_houle: f32,

    /// De combien le front des deux calottes ondule, en `sin(latitude)`.
    ///
    /// Un front parfaitement circulaire laisse voir la règle. Celui-ci se lit
    /// sur le Worley déjà échantillonné, à grande échelle — aucun bruit de
    /// plus. Voir [`Relief::onde`], qui l'écrête.
    pub front_onde: f32,
}

/// Un masque de région : son échelle en blocs, et où il bascule.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Masque {
    /// Taille des taches, en blocs.
    pub echelle: f64,
    /// Début et fin du palier : le masque vaut 1 au-delà, 0 en deçà.
    pub bas: f64,
    pub haut: f64,
}

/// Les seuils de la loi des biomes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Seuils {
    /// Altitude et chaos au-dessus desquels on parle de montagne.
    pub montagne_alt: f32,
    pub montagne_chaos: f32,
    /// Densité d'arbres sous laquelle une hauteur est nue.
    pub montagne_arbres: f32,

    pub desert_temp: f32,
    pub desert_humidite: f32,

    /// **Le seuil du désert se lit contre l'évaporation, jamais seul.** Tant
    /// que la chaleur asséchait jusqu'à zéro, 0,15 suffisait — tout ce qui
    /// était chaud tombait dessous. Avec un plancher, l'humidité ne descend
    /// plus si bas, et un désert doit se mériter par une humidité *brute*
    /// faible. Les deux nombres bougent ensemble ou pas du tout.
    ///
    /// La jungle demande les trois à la fois, et c'est ce qui la rend fragile.
    pub jungle_arbres: f32,
    pub jungle_humidite: f32,
    pub jungle_temp: f32,

    pub foret_arbres: f32,

    /// Plat, bas et gorgé d'eau : ce que demande un marais ordinaire.
    pub fond_humidite: f32,
    pub fond_chaos: f32,
    /// Hauteur au-dessus du niveau de la mer.
    pub fond_hauteur: f32,

    /// Le miasme demande beaucoup moins : un prédicat éparpillé intersecté
    /// avec un masque lisse ne donne pas une région, il donne des miettes.
    pub miasme_humidite: f32,
    pub miasme_chaos: f32,
    pub miasme_hauteur: f32,
}

/// Tout ce qui décide de la carte des biomes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reglages {
    pub bandes: Bandes,
    pub etagement: Etagement,
    pub evaporation: Evaporation,
    pub calottes: Calottes,
    pub relief: Relief,
    pub volcan: Masque,
    pub arcane: Masque,
    pub miasme: Masque,
    pub seuils: Seuils,
}

impl Default for Reglages {
    fn default() -> Self { DEFAUT }
}

/// Les valeurs que le jeu emploie.
///
/// **Elles ont ete choisies a la fenetre, pas au jugement.** Trois d'entre
/// elles se tiennent : l'evaporation, le seuil d'humidite du desert et sa
/// temperature. Baisser la seule temperature ne deplacait que 221 cases ;
/// c'etait l'humidite qui bridait. Monter la seule humidite mangeait la savane,
/// qui occupe la meme bande. Il a fallu les deux, et il a fallu les voir
/// bouger ensemble.
///
/// En `const` pour que `get_biome()` sans argument n'ait rien à allouer : il
/// est appelé depuis le client, `civ`, la faune, les spots et l'économie.
pub const DEFAUT: Reglages = Reglages {
    bandes: Bandes {
        poids: [3.0, 1.5, 0.75],
        onde: 0.10,
    },
    etagement: Etagement {
        gradient: 1.2 / 1000.0,
        halo: 1.2,
    },
    evaporation: Evaporation {
        seuil: 0.60,
        pente: 1.2,
        plancher: 0.80,
    },
    calottes: Calottes {
        banquise: 0.93,
        barriere: 0.72,
        abysse: 100.0,
    },
    // **Les deux seuils sortent des quantiles mesurés, pas du jugement.** Sur
    // un monde cubique de 128 chunks par face, la jointure va de -0,67 à 0,49,
    // médiane -0,15, q75 0,06, q95 0,36. Posés a priori a -0,30 et 0,00, ils
    // faisaient des trois quarts de la calotte une crete et d'aucune part une
    // crevasse. Ils sont maintenant lus sur la distribution : la crete occupe
    // le dernier quart, la fente le dernier vingtieme.
    relief: Relief {
        floe_taille: 220.0,
        floe_devers: 3.0,
        crete_seuil: 0.06,
        crete_hauteur: 5.0,
        crevasse_seuil: 0.30,
        crevasse_largeur: 0.10,
        crevasse_profondeur: 12.0,
        crevasse_plancher: 3.0,
        crevasse_fermeture: 0.75,
        banquise_francbord: 3.0,
        banquise_tirant: 13.0,
        barriere_francbord: 22.0,
        barriere_tirant: 40.0,
        barriere_houle: 2.0,
        front_onde: 0.008,
    },
    volcan: Masque {
        echelle: 1_500.0,
        bas: 0.05,
        haut: 0.25,
    },
    arcane: Masque {
        echelle: 2_200.0,
        bas: 0.30,
        haut: 0.50,
    },
    miasme: Masque {
        echelle: 2_600.0,
        bas: 0.05,
        haut: 0.22,
    },
    seuils: Seuils {
        montagne_alt: 500.0,
        montagne_chaos: 0.3,
        montagne_arbres: 0.6,
        desert_temp: 0.75,
        desert_humidite: 0.54,
        jungle_arbres: 0.65,
        jungle_humidite: 0.65,
        jungle_temp: 0.45,
        foret_arbres: 0.4,
        fond_humidite: 0.45,
        fond_chaos: 0.3,
        fond_hauteur: 80.0,
        miasme_humidite: 0.42,
        miasme_chaos: 0.55,
        miasme_hauteur: 400.0,
    },
};

impl Masque {
    /// Le masque en ce point, entre 0 et 1.
    ///
    /// Le seuil est adouci et jamais franc : un masque tout ou rien donnerait
    /// au halo géothermique un bord net.
    #[inline]
    pub fn palier(&self, bruit: f64) -> f32 {
        let t = ((bruit - self.bas) / (self.haut - self.bas)).clamp(0.0, 1.0);
        (t * t * (3.0 - 2.0 * t)) as f32
    }
}

impl Evaporation {
    /// La part d'humidité qui reste, à cette température.
    #[inline]
    pub fn facteur(&self, temp: f32) -> f32 {
        (1.0 - (temp - self.seuil).max(0.0) / self.pente).max(self.plancher)
    }
}

/// Un palier adouci de `bas` à `haut`, entre 0 et 1.
///
/// Le même que celui de [`Masque::palier`], sorti pour que le relief s'en
/// serve : un seuil franc y donnerait une marche là où on veut une pente.
#[inline]
pub fn lissage(bas: f32, haut: f32, x: f32) -> f32 {
    let t = ((x - bas) / (haut - bas)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Relief {
    /// L'ondulation du front, **écrêtée pour tenir dans la face polaire**.
    ///
    /// Un réglage qu'on peut bouger doit refuser de casser : le front de la
    /// barrière commence à 0,72 et le plancher dur est à 0,707, ce qui ne
    /// laisse que treize millièmes. Un curseur poussé plus loin ferait
    /// enjamber les quatre coutures de la face à l'anneau du front, et le
    /// défaut n'y suffit pas — c'est ici que ça se refuse, pas dans `DEFAUT`.
    #[inline]
    pub fn onde(&self, calottes: &Calottes) -> f32 {
        let marge = (calottes.banquise.min(calottes.barriere) - PLANCHER_FACE).max(0.0);
        self.front_onde.clamp(0.0, marge)
    }

    /// L'épaisseur totale de la banquise, en blocs.
    #[inline]
    pub fn banquise_epaisseur(&self) -> f32 { self.banquise_francbord + self.banquise_tirant }
}

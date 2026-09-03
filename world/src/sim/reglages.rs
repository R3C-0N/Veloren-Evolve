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

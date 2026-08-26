//! La projection du cube sur la sphère.
//!
//! Poser une face carrée sur une sphère déforme ses cases ; c'est la taxe de
//! Gauss-Bonnet, et elle se paye soit en forme, soit en taille. Deux régimes
//! purs existent, et aucun n'est bon partout :
//!
//! | | forme d'une case | taille d'une case |
//! |---|---|---|
//! | **Équiangulaire** | losange à 120° près des coins | uniforme à ±8 % |
//! | **Conforme** | carrée partout | s'effondre vers zéro aux coins |
//!
//! Ce module prend la conforme **et la raccorde à l'équiangulaire dans un petit
//! disque autour de chaque coin**. Les cases restent donc carrées sur
//! l'immense majorité du monde, et là où la conformité coûterait des blocs si
//! petits qu'ils se confondent, on lui reprend la main pour leur rendre leur
//! taille. Le cisaillement revient, mais confiné à huit pastilles.
//!
//! ## La carte conforme
//!
//! En coordonnée stéréographique `ζ` prise depuis le centre de la face, les
//! huit coins du cube sont les racines de
//!
//! ```text
//! Φ(ζ) = ζ⁸ + 14ζ⁴ + 1
//! ```
//!
//! La surface plate du cube n'offre que 270° à chacun de ces coins, contre 360°
//! sur la sphère : la carte doit donc s'y comporter en puissance 3/4. C'est un
//! Schwarz-Christoffel, et il s'écrit d'un trait :
//!
//! ```text
//! dζ/dw = K · Φ(ζ)^(1/4)
//! ```
//!
//! On intègre cette équation depuis le centre de la face — Runge-Kutta 4, une
//! colonne puis chaque ligne — et on tabule le résultat. La table est partagée
//! **octet pour octet** avec le vertex shader : le rendu et la logique doivent
//! voir exactement la même planète, sans quoi la visée dériverait.
//!
//! ## Ce qui est tabulé
//!
//! Pas `ζ`, mais les coordonnées du **plan tangent gnomonique** `(a, b)`, d'où
//! la direction se tire par `normalize(n + r·a + h·b)`. Ce choix n'est pas de
//! commodité : le bord d'une face y est exactement `a = ±1`, si bien que
//! mélanger deux cartes dans ces coordonnées laisse le bord **sur l'arête du
//! cube**. Le même mélange fait sur `ζ` couperait la corde et ouvrirait les
//! recollements.

use std::sync::OnceLock;

/// Côté de la table. Impair : le centre de la face doit être un point tabulé.
pub const N: usize = 513;

/// `K = ∫₀^{√2−1} dy / (y⁸ + 14y⁴ + 1)^{1/4}`, l'intégrale le long de l'axe
/// imaginaire jusqu'au milieu d'arête. Calculée une fois, à part.
pub const K: f64 = 0.406_683_250_145_049_9;

/// Rayon du raccord, en coordonnées de face — `2` est le côté d'une face.
///
/// Au-delà, la carte est purement conforme. En deçà, elle glisse vers
/// l'équiangulaire pour que les blocs cessent de rétrécir.
///
/// La valeur se choisit sur une mesure, pas au jugé, parce que les deux bouts
/// sont mauvais. Trop étroit, le poids varie si vite que le terme de raccord
/// domine la dérivée et écrase les cases dans un anneau ; trop large, le
/// losange gagne une bonne part de la face. `--diag` mesure les deux, et
/// `PROTO_RACCORD` permet de refaire le tour :
///
/// | rayon | côté de case le plus court | zone en losange |
/// |---|---|---|
/// | 0,15 | 0,24 | 220 blocs |
/// | **0,25** | **0,42** | **364 blocs** |
/// | 0,35 | 0,55 | 496 blocs |
/// | 0,50 | 0,66 | 674 blocs |
pub fn raccord() -> f64 {
    static R: OnceLock<f64> = OnceLock::new();
    *R.get_or_init(|| {
        std::env::var("PROTO_RACCORD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.25)
    })
}

type C = (f64, f64);

fn mul(a: C, b: C) -> C { (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0) }
fn add(a: C, b: C) -> C { (a.0 + b.0, a.1 + b.1) }
fn ech(a: C, k: f64) -> C { (a.0 * k, a.1 * k) }

/// `K · Φ(ζ)^{1/4}`, sur la branche principale.
///
/// Elle suffit : sur toute la face, `arg Φ` reste dans `±2,10 rad`, donc bien
/// en deçà de la coupure. C'est vérifié, pas supposé — sans quoi il faudrait
/// suivre la branche pas à pas.
fn derivee(z: C) -> C {
    let z2 = mul(z, z);
    let z4 = mul(z2, z2);
    let z8 = mul(z4, z4);
    let phi = (z8.0 + 14.0 * z4.0 + 1.0, z8.1 + 14.0 * z4.1);

    let module = (phi.0 * phi.0 + phi.1 * phi.1).sqrt().sqrt().sqrt();
    let angle = phi.1.atan2(phi.0) / 4.0;
    (K * module * angle.cos(), K * module * angle.sin())
}

fn pas_rk4(z: C, dw: C) -> C {
    let k1 = derivee(z);
    let k2 = derivee(add(z, ech(mul(dw, k1), 0.5)));
    let k3 = derivee(add(z, ech(mul(dw, k2), 0.5)));
    let k4 = derivee(add(z, mul(dw, k3)));
    let somme = (
        k1.0 + 2.0 * k2.0 + 2.0 * k3.0 + k4.0,
        k1.1 + 2.0 * k2.1 + 2.0 * k3.1 + k4.1,
    );
    add(z, ech(mul(dw, somme), 1.0 / 6.0))
}

/// `ζ` → plan tangent gnomonique.
fn gnomonique((p, q): C) -> C {
    let d = 1.0 + p * p + q * q;
    let (x, y, z) = (2.0 * p / d, 2.0 * q / d, (1.0 - p * p - q * q) / d);
    (x / z, y / z)
}

/// La carte équiangulaire : celle qui garde la taille et perd la forme.
fn equiangulaire(s: f64, t: f64) -> C {
    let quart = std::f64::consts::FRAC_PI_4;
    ((s * quart).tan(), (t * quart).tan())
}

/// Poids du conforme : `1` loin des coins, `0` au coin, avec un raccord doux.
fn poids(s: f64, t: f64) -> f64 {
    let d = ((1.0 - s.abs()).powi(2) + (1.0 - t.abs()).powi(2)).sqrt();
    let x = (d / raccord()).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

pub struct Table {
    /// `(a, b)` gnomonique pour chaque `(s, t)` de la grille. En `f32` : c'est
    /// exactement ce que le shader lira, et les deux doivent voir la même chose.
    ab: Vec<[f32; 2]>,
    /// `ζ` au coin, avant raccord : sert à juger l'intégration numérique.
    pub coin_brut: C,
}

static TABLE: OnceLock<Table> = OnceLock::new();

pub fn table() -> &'static Table { TABLE.get_or_init(Table::construire) }

impl Table {
    fn construire() -> Self {
        let h = 2.0 / (N - 1) as f64;
        let milieu = (N - 1) / 2;

        // La colonne centrale, en remontant puis en descendant depuis `ζ = 0`.
        let mut colonne = vec![(0.0, 0.0); N];
        let mut z = (0.0, 0.0);
        for c in colonne.iter_mut().skip(milieu + 1) {
            z = pas_rk4(z, (0.0, h));
            *c = z;
        }
        z = (0.0, 0.0);
        for c in colonne.iter_mut().take(milieu).rev() {
            z = pas_rk4(z, (0.0, -h));
            *c = z;
        }

        // Puis chaque ligne, à partir d'elle. On raccorde au passage.
        let mut ab = vec![[0.0f32; 2]; N * N];
        let mut pose = |i: usize, j: usize, zeta: C| {
            let (s, t) = (-1.0 + 2.0 * i as f64 / (N - 1) as f64, -1.0 + 2.0 * j as f64 / (N - 1) as f64);
            let conf = gnomonique(zeta);
            let equi = equiangulaire(s, t);
            let l = poids(s, t);
            ab[j * N + i] = [
                (l * conf.0 + (1.0 - l) * equi.0) as f32,
                (l * conf.1 + (1.0 - l) * equi.1) as f32,
            ];
        };

        for (j, depart) in colonne.iter().enumerate() {
            pose(milieu, j, *depart);
            let mut z = *depart;
            for i in milieu + 1..N {
                z = pas_rk4(z, (h, 0.0));
                pose(i, j, z);
            }
            let mut z = *depart;
            for i in (0..milieu).rev() {
                z = pas_rk4(z, (-h, 0.0));
                pose(i, j, z);
            }
        }

        // Le coin, refait à part pour pouvoir le confronter à la théorie.
        let mut coin = (0.0, 0.0);
        for _ in 0..4096 {
            coin = pas_rk4(coin, (1.0 / 4096.0, 1.0 / 4096.0));
        }

        Self { ab, coin_brut: coin }
    }

    /// `(a, b)` interpolé, pour `(s, t)` dans `[−1, 1]²`.
    ///
    /// L'interpolation bilinéaire est reproduite à l'identique dans le shader.
    pub fn ab(&self, s: f64, t: f64) -> C {
        let n = (N - 1) as f64;
        let x = ((s + 1.0) * 0.5 * n).clamp(0.0, n - 1e-9);
        let y = ((t + 1.0) * 0.5 * n).clamp(0.0, n - 1e-9);
        let (i, j) = (x as usize, y as usize);
        let (fx, fy) = (x - i as f64, y - j as f64);

        let lis = |i: usize, j: usize| {
            let v = self.ab[j * N + i];
            (v[0] as f64, v[1] as f64)
        };
        let (a, b, c, d) = (lis(i, j), lis(i + 1, j), lis(i, j + 1), lis(i + 1, j + 1));
        let bas = (a.0 + (b.0 - a.0) * fx, a.1 + (b.1 - a.1) * fx);
        let haut = (c.0 + (d.0 - c.0) * fx, c.1 + (d.1 - c.1) * fx);
        (bas.0 + (haut.0 - bas.0) * fy, bas.1 + (haut.1 - bas.1) * fy)
    }

    /// Direction unité dans le repère de la face — `x` le long de `r`, `y` le
    /// long de `h`, `z` le long de la normale.
    pub fn direction_locale(&self, s: f64, t: f64) -> [f64; 3] {
        let (a, b) = self.ab(s, t);
        let l = (1.0 + a * a + b * b).sqrt();
        [a / l, b / l, 1.0 / l]
    }

    /// L'inverse, par Newton.
    ///
    /// Le jacobien se prend **sur la table**, par différences finies, et non sur
    /// la formule conforme : dans le raccord, la carte n'est plus holomorphe et
    /// une dérivée analytique y mentirait.
    pub fn depuis_locale(&self, d: [f64; 3]) -> (f64, f64) {
        let cible = (d[0] / d[2], d[1] / d[2]);
        let quart = std::f64::consts::FRAC_PI_4;

        // Départ : l'estimation équiangulaire, qui n'est jamais très loin.
        let mut w = (
            (cible.0.atan() / quart).clamp(-1.0, 1.0),
            (cible.1.atan() / quart).clamp(-1.0, 1.0),
        );
        let pas = 2.0 / (N - 1) as f64;

        for _ in 0..12 {
            let p = self.ab(w.0, w.1);
            let e = (cible.0 - p.0, cible.1 - p.1);
            if e.0.abs() + e.1.abs() < 1e-13 {
                break;
            }

            let (ds, js) = derive_table(self, w, p, pas, true);
            let (dt, jt) = derive_table(self, w, p, pas, false);
            let _ = (ds, dt);
            let det = js.0 * jt.1 - jt.0 * js.1;
            if det.abs() < 1e-15 {
                break;
            }

            let mut delta = (
                (e.0 * jt.1 - e.1 * jt.0) / det,
                (e.1 * js.0 - e.0 * js.1) / det,
            );
            let long = (delta.0 * delta.0 + delta.1 * delta.1).sqrt();
            if long > 0.25 {
                delta = ech(delta, 0.25 / long);
            }
            w = (
                (w.0 + delta.0).clamp(-1.0, 1.0),
                (w.1 + delta.1).clamp(-1.0, 1.0),
            );
        }
        w
    }

    /// La table telle que le shader la lira : lignes complétées au multiple de
    /// 256 octets qu'exige la copie vers une texture.
    pub fn octets(&self) -> (Vec<u8>, u32) {
        let brut = N * 8;
        let pas = brut.div_ceil(256) * 256;
        let mut octets = vec![0u8; pas * N];
        for j in 0..N {
            for i in 0..N {
                let v = self.ab[j * N + i];
                let o = j * pas + i * 8;
                octets[o..o + 4].copy_from_slice(&v[0].to_le_bytes());
                octets[o + 4..o + 8].copy_from_slice(&v[1].to_le_bytes());
            }
        }
        (octets, pas as u32)
    }
}

/// Une colonne du jacobien, par différence finie décalée vers l'intérieur quand
/// on est au bord de la table.
fn derive_table(table: &Table, w: C, p: C, pas: f64, selon_s: bool) -> (f64, C) {
    let (avant, signe) = if selon_s {
        if w.0 + pas <= 1.0 { ((w.0 + pas, w.1), 1.0) } else { ((w.0 - pas, w.1), -1.0) }
    } else if w.1 + pas <= 1.0 {
        ((w.0, w.1 + pas), 1.0)
    } else {
        ((w.0, w.1 - pas), -1.0)
    };
    let q = table.ab(avant.0, avant.1);
    (pas, ((q.0 - p.0) * signe / pas, (q.1 - p.1) * signe / pas))
}

/// Où doit tomber un coin de face en `ζ` : `(1 + i)/(√3 + 1)`. `--diag` s'en
/// sert pour juger l'intégration plutôt que de la croire sur parole.
pub fn coin_attendu() -> C {
    let c = 1.0 / (3.0f64.sqrt() + 1.0);
    (c, c)
}

"""Genere les modeles .vox des blocs ramassables (common.items.block.*).

MagicaVoxel v150 : MAIN { SIZE, XYZI, RGBA }. L'indice de couleur d'un voxel
vaut `position dans la palette + 1` — l'indice 0 signifie « vide ».

Les huit modeles sont de petits cubes de 7 voxels d'arete, un par materiau, avec
un relief et un mouchetis propres a chacun. Deterministe : meme graine, memes
fichiers, pour que regenerer ne fasse pas de diff parasite.

    python .claude/skills/run-veloren-evolve/gen_block_vox.py

Ecrit dans assets/voxygen/voxel/item/block/.
"""

import os
import random
import struct

N = 7  # arete du cube, en voxels


def chunk(cid: bytes, content: bytes, children: bytes = b"") -> bytes:
    return cid + struct.pack("<ii", len(content), len(children)) + content + children


def ecrire_vox(chemin: str, voxels: dict, palette: list) -> None:
    """voxels : {(x, y, z): index_palette_0_base}. palette : [(r, g, b), ...]."""
    size = chunk(b"SIZE", struct.pack("<iii", N, N, N))
    corps = b"".join(
        struct.pack("<BBBB", x, y, z, idx + 1) for (x, y, z), idx in sorted(voxels.items())
    )
    xyzi = chunk(b"XYZI", struct.pack("<i", len(voxels)) + corps)
    pal = list(palette) + [(0, 0, 0)] * (256 - len(palette))
    rgba = chunk(b"RGBA", b"".join(struct.pack("<BBBB", r, g, b, 255) for r, g, b in pal[:256]))
    main = chunk(b"MAIN", b"", size + xyzi + rgba)
    with open(chemin, "wb") as f:
        f.write(b"VOX " + struct.pack("<i", 150) + main)


def cube(rng, palette, choisir, creuser=None, ebrecher=0.0):
    """Construit un cube plein, puis retire des voxels de coin/surface.

    `choisir(x, y, z, dessus)` rend l'indice de palette ; `creuser` peut rendre
    True pour laisser un trou ; `ebrecher` est la probabilite d'oter un voxel
    d'arete, ce qui casse la silhouette trop parfaite d'un cube.
    """
    v = {}
    for x in range(N):
        for y in range(N):
            for z in range(N):
                bord = x in (0, N - 1) or y in (0, N - 1) or z in (0, N - 1)
                if not bord:
                    continue  # coque creuse : invisible de l'exterieur, et 5x moins de voxels
                aretes = (x in (0, N - 1)) + (y in (0, N - 1)) + (z in (0, N - 1))
                if aretes >= 2 and rng.random() < ebrecher:
                    continue
                if creuser is not None and creuser(x, y, z, rng):
                    continue
                v[(x, y, z)] = choisir(x, y, z, z == N - 1, rng)
    return v


def melange(base, variantes, rng, p=0.28):
    return rng.choice(variantes) if rng.random() < p else base


def main() -> None:
    racine = os.path.join(os.path.dirname(__file__), "..", "..", "..")
    sortie = os.path.abspath(os.path.join(racine, "assets", "voxygen", "voxel", "item", "block"))
    os.makedirs(sortie, exist_ok=True)

    modeles = {}

    # --- pierre : gris, mouchetis sombre, aretes ebrechees -----------------
    pal = [(122, 122, 128), (100, 100, 107), (142, 142, 148), (86, 86, 92)]
    modeles["stone"] = (
        pal,
        lambda rng: cube(rng, pal, lambda x, y, z, top, r: melange(0, [1, 2, 3], r, 0.34),
                         ebrecher=0.30),
    )

    # --- terre : brun, quelques cailloux plus sombres ----------------------
    pal = [(104, 74, 50), (86, 60, 40), (122, 90, 62), (70, 50, 34)]
    modeles["earth"] = (
        pal,
        lambda rng: cube(rng, pal, lambda x, y, z, top, r: melange(0, [1, 2, 3], r, 0.36),
                         ebrecher=0.22),
    )

    # --- sable : jaune pale, grain fin, arete tres emoussee ----------------
    pal = [(214, 194, 140), (198, 176, 124), (226, 210, 162)]
    modeles["sand"] = (
        pal,
        lambda rng: cube(rng, pal, lambda x, y, z, top, r: melange(0, [1, 2], r, 0.45),
                         ebrecher=0.45),
    )

    # --- gazon : terre dessous, herbe dessus, brins qui debordent ----------
    pal = [(104, 74, 50), (86, 60, 40), (74, 132, 54), (94, 158, 66), (60, 112, 44)]

    def gazon(x, y, z, top, r):
        if z >= N - 2:
            return melange(2, [3, 4], r, 0.45)
        return melange(0, [1], r, 0.3)

    modeles["grass"] = (pal, lambda rng: cube(rng, pal, gazon, ebrecher=0.16))

    # --- bois : fil vertical, bout de bille plus clair dessus --------------
    pal = [(126, 92, 56), (104, 74, 44), (146, 110, 70), (166, 132, 90)]

    def bois(x, y, z, top, r):
        if top:
            return 3 if (x + y) % 3 else 2  # bout de bille, cernes clairs
        return 1 if x % 3 == 0 else melange(0, [2], r, 0.22)

    modeles["wood"] = (pal, lambda rng: cube(rng, pal, bois, ebrecher=0.08))

    # --- feuillage : deux verts, troue ------------------------------------
    pal = [(64, 116, 46), (84, 142, 58), (48, 92, 38)]
    modeles["leaves"] = (
        pal,
        lambda rng: cube(
            rng, pal,
            lambda x, y, z, top, r: melange(0, [1, 2], r, 0.5),
            creuser=lambda x, y, z, r: r.random() < 0.16,
            ebrecher=0.35,
        ),
    )

    # --- neige : blanc casse, quelques eclats plus bleus -------------------
    pal = [(238, 242, 248), (222, 230, 240), (250, 252, 255)]
    modeles["snow"] = (
        pal,
        lambda rng: cube(rng, pal, lambda x, y, z, top, r: melange(0, [1, 2], r, 0.34),
                         ebrecher=0.38),
    )

    # --- glace : cyan pale, reflets clairs ---------------------------------
    pal = [(168, 208, 226), (146, 190, 214), (198, 230, 244)]
    modeles["ice"] = (
        pal,
        lambda rng: cube(rng, pal, lambda x, y, z, top, r: melange(0, [1, 2], r, 0.30),
                         ebrecher=0.12),
    )

    for i, (nom, (pal, faire)) in enumerate(sorted(modeles.items())):
        rng = random.Random(1000 + i)  # graine fixe : sortie reproductible
        v = faire(rng)
        chemin = os.path.join(sortie, nom + ".vox")
        ecrire_vox(chemin, v, pal)
        print("%-8s %4d voxels  %s" % (nom, len(v), chemin))


if __name__ == "__main__":
    main()

# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (c) 2026 the OpenRAC contributors
"""
The map the port reads the disc by.

The games read their data by absolute sector, most of it outside the file system. The extractor
keeps the disc image in iso_data/GAME/disc.iso and writes the sector map, sectors.tsv, beside it:

  # openrac sector map 1: first sector, sectors, bytes, file
  0	2072064	4243587072	disc.iso

The port (port/assets/disc/sector_map.h) gives the game the bytes at any sector it asks for
through the map. A map may name other files (set-ups made when the extractor wrote the disc out
as files; the launcher's tidy turns them back into this one row): a row's file holds BYTES bytes,
its sectors past them read as zeros, and the rows cover every sector, in order.

Up Your Arsenal's two programs (its boot executable and I5BOOTN.ELF, the multiplayer game) are a
loader carrying the game WAD-compressed at file offset 0x7080: they are unpacked here, into
programs/<file>.records (the loader's records, as port/game/rac3-pal/host/exec.c copies them to
memory), so the port never unpacks a program from the disc at run time.
"""
import sys
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import BinaryIO

SECTOR = 2048
MAP_NAME = "sectors.tsv"
MAP_VERSION = 1
MAP_HEADER = f"# openrac sector map {MAP_VERSION}: first sector, sectors, bytes, file"
PROGRAMS = "programs"
COPY_BLOCK = 1 << 23

# Up Your Arsenal's loaders: the packed game at file offset 0x7080 (port/game/rac3-pal/host/exec.c).
RAC3_PAYLOAD_OFFSET = 0x7080
RAC3_PROGRAMS = ("I5BOOTN.ELF",)


@dataclass(frozen=True)
class Piece:
    """One row of the map: SECTORS sectors from FIRST, in PATH (relative), which holds BYTES."""
    first: int
    sectors: int
    bytes: int
    path: str


def _editor() -> Path | None:
    """The checkout's (or the standalone copy's) extractor/, whose readers know the tables of contents."""
    here = Path(__file__).resolve().parents[2] / "extractor"
    return here if (here / "disc.py").is_file() else None


def copy_range(src: BinaryIO, offset: int, size: int, dest: Path) -> None:
    """SIZE bytes of SRC at OFFSET into DEST."""
    dest.parent.mkdir(parents=True, exist_ok=True)
    src.seek(offset)
    left = size
    with open(dest, "wb") as out:
        while left:
            block = src.read(min(left, COPY_BLOCK))
            if not block:
                raise ValueError(f"{dest.name} runs past the end of the image")
            out.write(block)
            left -= len(block)


def image_run(image: Path, name: str | None = None) -> Piece:
    """The one row of a map that reads every sector from IMAGE, kept beside the map as NAME (its
    own name when None)."""
    sectors = Path(image).stat().st_size // SECTOR
    return Piece(0, sectors, sectors * SECTOR, name or Path(image).name)


def write_map(rows: list[Piece], out: Path) -> Path:
    """OUT/sectors.tsv for ROWS."""
    path = out / MAP_NAME
    lines = [MAP_HEADER] + [f"{p.first}\t{p.sectors}\t{p.bytes}\t{p.path}" for p in rows]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return path


def read_map(folder: Path) -> list[Piece]:
    """The rows of FOLDER/sectors.tsv (ValueError when it is not a map of this version)."""
    text = (folder / MAP_NAME).read_text(encoding="utf-8").splitlines()
    if not text or text[0] != MAP_HEADER:
        raise ValueError(f"{folder / MAP_NAME} is not a sector map of version {MAP_VERSION}")
    rows = []
    for line in text[1:]:
        if line and not line.startswith("#"):
            first, count, size, path = line.split("\t", 3)
            rows.append(Piece(int(first), int(count), int(size), path))
    return rows


def unpack_programs(game: str, out: Path, log: Callable[[str], None] = print,
                    boot: str | None = None) -> list[str]:
    """Up Your Arsenal's programs unpacked into OUT/programs/<file>.records (the module comment):
    the boot executable BOOT and I5BOOTN.ELF, from their extracted files in OUT. Nothing for the
    other games. Raises ValueError when a loader holds no packed game."""
    if game != "rac3":
        return []
    editor = _editor()
    if editor is None:
        raise ValueError("Up Your Arsenal's programs need the extractor's WAD reader (extractor/formats.py)")
    if str(editor) not in sys.path:
        sys.path.insert(1, str(editor))
    from formats import wad  # noqa: PLC0415
    done = []
    for name in (*([boot] if boot else []), *RAC3_PROGRAMS):
        src = out / name
        if not src.is_file():
            continue
        data = src.read_bytes()
        if len(data) <= RAC3_PAYLOAD_OFFSET + 16 or data[RAC3_PAYLOAD_OFFSET:RAC3_PAYLOAD_OFFSET + 3] != b"WAD":
            raise ValueError(f"{name} carries no packed game at {RAC3_PAYLOAD_OFFSET:#x}")
        records = wad(data[RAC3_PAYLOAD_OFFSET:])
        dest = out / PROGRAMS / f"{name}.records"
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(records)
        log(f"  {PROGRAMS}/{name}.records: {len(records)} bytes, unpacked from {name}")
        done.append(name)
    return done


def write_disc(image: Path, out: Path, game: str, boot: str | None = None,
               log: Callable[[str], None] = print) -> list[Piece]:
    """What the port reads of IMAGE, for a set-up in OUT that keeps the image (the iOS
    importer's): GAME's programs and the map, written last, which reads every sector from IMAGE
    (no copy of the disc). The boot executable BOOT must be in OUT already."""
    image = Path(image)
    if image.parent.resolve() != Path(out).resolve():
        raise ValueError(f"{image} is not in {out}: the map reads the image beside it")
    for name in RAC3_PROGRAMS if game == "rac3" else ():
        from openrac_common import iso9660  # noqa: PLC0415 (iso9660 imports nothing of this)
        for path, lba, size in iso9660.files(image):
            if path == name and not (out / path).is_file():
                with open(image, "rb") as f:
                    copy_range(f, lba * SECTOR, size, out / path)
    unpack_programs(game, out, log, boot=boot)
    rows = [image_run(image)]
    write_map(rows, out)
    return rows

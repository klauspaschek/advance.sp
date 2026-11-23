#!/usr/bin/env python3
"""Validate an ARM64 card tree and produce the public release downloads."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import zipfile


def package(root: Path, output: Path, source: str) -> None:
    if not re.fullmatch(r'[0-9a-f]{40}', source):
        raise ValueError('source must be a full Git commit SHA')
    root = root.resolve()
    output = output.resolve()
    if output == root or root in output.parents:
        raise ValueError('output must be outside the card tree')
    system = root / 'System'
    for name in ('advance', 'mgba_libretro.so', 'gpsp_libretro.so', 'fsck.exfat'):
        binary = system / name
        header = binary.read_bytes()[:20]
        if len(header) < 20 or header[:6] != b'\x7fELF\x02\x01' or int.from_bytes(header[18:20], 'little') != 183:
            raise ValueError(f'{name} must be a little-endian ELF64 AArch64 binary')
    for name in ('smbd', 'smbpasswd', 'testparm', 'wsdd2', 'samba-dcerpcd', 'rpcd_classic', 'rpcd_winreg'):
        binary = system / 'samba/bin' / name
        header = binary.read_bytes()[:20]
        if header[:6] != b'\x7fELF\x02\x01' or int.from_bytes(header[18:20], 'little') != 183:
            raise ValueError(f'Samba {name} must be an AArch64 binary')
        if not binary.stat().st_mode & 0o111:
            raise ValueError(f'Samba {name} must be executable')
    if not (system / 'samba/libexec/rpcd_classic').stat().st_mode & 0o111:
        raise ValueError('missing executable private RPC worker wrapper')
    if not (system / 'samba/libexec/rpcd_winreg').stat().st_mode & 0o111:
        raise ValueError('missing executable private registry RPC worker wrapper')
    if not (system / 'samba/lib/ld-linux-aarch64.so.1').is_file():
        raise ValueError('missing private Samba loader')
    if not (system / 'advance').stat().st_mode & 0o111:
        raise ValueError('System/advance must be executable')
    if not (system / 'wifi.sh').stat().st_mode & 0o111:
        raise ValueError('Wi-Fi DHCP callback must be executable')
    licenses = system / 'licenses'
    if not any(licenses.glob('gpsp-*.tar.gz')) or not any(licenses.glob('gpsp-*.meta')) or not any(licenses.glob('mgba-*.meta')):
        raise ValueError('missing emulator source archive or provenance')
    if not any(licenses.glob('gpsp-*.patch')) or not any(licenses.glob('mgba-*.patch')):
        raise ValueError('missing emulator source patches')
    if not any(licenses.glob('exfatprogs-*.tar.gz')) or not (licenses / 'exfatprogs-GPL-2.0.txt').is_file():
        raise ValueError('missing SD repair tool source or license')
    metadata = {
        'product': 'advance.sp',
        'repository': os.environ.get('GITHUB_REPOSITORY', 'klauspaschek/advance.sp'),
        'source_commit': source,
        'architecture': 'aarch64',
        'run_url': f'https://github.com/{os.environ.get("GITHUB_REPOSITORY", "klauspaschek/advance.sp")}/actions/runs/{os.environ.get("GITHUB_RUN_ID", "local")}',
    }
    (system / 'build-info.json').write_text(json.dumps(metadata, indent=2) + '\n')
    output.mkdir(parents=True, exist_ok=True)
    (output / 'build-info.json').write_bytes((system / 'build-info.json').read_bytes())
    for filename, subtree in [('advance.sp-System.zip', system), ('advance.sp-SD2.zip', root)]:
        with zipfile.ZipFile(output / filename, 'w', zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(subtree.rglob('*')):
                if path.is_symlink():
                    raise ValueError(f'unexpected symlink: {path}')
                archive.write(path, path.relative_to(root))
        with zipfile.ZipFile(output / filename) as archive:
            if archive.testzip() is not None:
                raise ValueError(f'corrupt archive: {filename}')
            if 'System/advance' not in archive.namelist():
                raise ValueError(f'incorrect archive layout: {filename}')
    # Remove aliases left by an earlier build when reusing an output directory.
    for name in ('slot-System.zip', 'slot-SD2.zip', 'slot.sp-System.zip', 'slot.sp-SD2.zip', 'slot.sp-network-sources.zip'):
        (output / name).unlink(missing_ok=True)
    sources = Path('vendor/samba-sources')
    if not (sources / 'dependency-sources.tsv').is_file():
        raise ValueError('missing network-sharing corresponding source')
    with zipfile.ZipFile(output / 'advance.sp-network-sources.zip', 'w', zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(sources.rglob('*')):
            if path.is_file(): archive.write(path, path.relative_to(sources))
    if (output / 'advance.sp-System.zip').stat().st_size > 64 * 1024 * 1024:
        raise ValueError('System archive exceeds the OTA download limit')
    files = ['advance.sp-network-sources.zip', 'advance.sp-System.zip', 'advance.sp-SD2.zip', 'build-info.json']
    (output / 'SHA256SUMS').write_text(''.join(
        f'{hashlib.sha256((output / name).read_bytes()).hexdigest()}  {name}\n' for name in files
    ))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('source')
    args = parser.parse_args()
    package(args.root, args.output, args.source)

from pathlib import Path
import hashlib
import os
import subprocess
import tarfile
import tempfile
import sys

root = Path(__file__).resolve().parent.parent
archive_path = Path(sys.argv[1]).resolve()
(root / '.work').mkdir(exist_ok=True)
work = Path(tempfile.mkdtemp(prefix='verify-', dir=root / '.work'))

def run(*args, cwd, ok=True):
    result = subprocess.run(args, cwd=cwd, text=True, capture_output=True)
    if ok and result.returncode:
        raise RuntimeError(f'{args!r}: {result.stdout}\n{result.stderr}')
    return result

def snapshot(directory):
    result = {}
    for path in directory.rglob('*'):
        relative = path.relative_to(directory)
        if '.git' in relative.parts:
            continue
        if path.is_symlink():
            result[str(relative)] = ('link', os.readlink(path))
        elif path.is_file():
            result[str(relative)] = (path.stat().st_mode & 0o777, hashlib.sha256(path.read_bytes()).hexdigest())
    return result

with tarfile.open(archive_path) as archive:
    archive.extractall(work, filter='data')
source = work / 'zed-source'
run('git', 'init', '-q', cwd=source)
run('git', 'add', '.', cwd=source)
run('git', '-c', 'user.name=Source verification', '-c', 'user.email=verification@localhost', 'commit', '-qm', 'Archived upstream source', cwd=source)
baseline = snapshot(source)
prepare = str(root / 'scripts/prepare-source.py')
run('python3', prepare, str(source), cwd=root)
modified = snapshot(source)
changed = {name for name in baseline.keys() | modified.keys() if baseline.get(name) != modified.get(name)}
expected = {'Cargo.lock', 'ios/script/cargo-build-ios', 'crates/zed_ios/src/lib.rs', 'crates/zed_ios/src/connection_landing.rs', 'crates/workspace/src/security_modal.rs', 'crates/zed_ios/src/authentication_prompt.rs', 'crates/remote/src/remote.rs', 'crates/remote/src/remote_client.rs', 'crates/remote/src/transport.rs', 'crates/remote/src/transport/russh_ssh.rs', 'crates/remote/src/transport/russh_auth.rs'}
assert changed == expected, changed
assert hashlib.sha256((source / 'Cargo.lock').read_bytes()).hexdigest() == 'e6405d8ef6142954e44955e49c4947587017ce5cd65778853fc3180816f0861a'
run('git', 'diff', '--check', cwd=source)
print('PASS: exact upstream archive accepts patch; only eleven intended inputs changed; lock hash unchanged', flush=True)

repeat = run('python3', prepare, str(source), cwd=root, ok=False)
assert repeat.returncode != 0
assert snapshot(source) == modified
print('PASS: repeated preparation fails without modifying inputs', flush=True)

# The packaging command must include new source files before staging their content.
patch = run('git', 'diff', '--binary', 'HEAD', cwd=source).stdout
patch_file = work / 'source.patch'
patch_file.write_text(patch)
restored = work / 'restored'
restored.mkdir()
with tarfile.open(archive_path) as archive:
    archive.extractall(restored, filter='data')
restored = restored / 'zed-source'
run('patch', '-p1', '-i', str(patch_file), cwd=restored)
assert snapshot(restored) == modified
run('git', 'add', 'crates/zed_ios/src', cwd=source)
assert run('git', 'diff', '--binary', 'HEAD', cwd=source).stdout == patch
print('PASS: archive + shipped patch restores every file, mode and symlink, including staged changes', flush=True)

# Deliberately change a patch context while keeping the Cargo phase pristine.
run('git', 'reset', '--hard', '-q', 'HEAD', cwd=source)
p = source / 'crates/remote/src/remote_client.rs'
p.write_text(p.read_text().replace('pub trait RemoteClientDelegate: Send + Sync {', 'pub trait ChangedRemoteClientDelegate: Send + Sync {'))
drifted = snapshot(source)
reject = run('python3', prepare, str(source), cwd=root, ok=False)
assert reject.returncode != 0
assert snapshot(source) == drifted
print('PASS: upstream drift fails before modifying the Cargo phase, lock or client files', flush=True)
print(f'Prepared source for protocol tests: {restored}', flush=True)

"""Set the version of the crates in the workspace.

Usage:

    # in a venv with version-requirements.txt installed
    # in the root of a workspace

    $ python3 resources/scripts/version.py --ver 0.0.6
"""

from absl import app
from absl import flags
from pathlib import Path
import tomlkit


FLAGS = flags.FLAGS


flags.DEFINE_string("ver", None, "The new version, e.g. 0.0.6")


def set_version(cargo_file: Path, workspace_members):
    new_version = [int(s) for s in FLAGS.ver.split(".")]
    assert len(new_version) == 3, f"Bad version {new_version}"
    new_version = ".".join(str(v) for v in new_version)

    assert cargo_file.is_file(), cargo_file
    toml = tomlkit.loads(cargo_file.read_text())

    if "workspace" in toml:
        version = toml["workspace"]["package"]["version"]
        version = [int(s) for s in version.split(".")]
        if all(v == 0 for v in version):
            return  # nop
        toml["workspace"]["package"]["version"] = new_version

    # bump dependencies on workspace targets, distinguished by use of path
    workspace_members = set(workspace_members)
    for dep_block in ["dependencies", "dev-dependencies"]:
        if dep_block not in toml:
            continue
        for dep_name, dep_cfg in toml[dep_block].items():
            if dep_name not in workspace_members:
                continue
            dep_cfg["version"] = new_version

    cargo_file.write_text(tomlkit.dumps(toml))


def main(_):
    workspace_file = Path("Cargo.toml")
    workspace = tomlkit.loads(workspace_file.read_text())

    workspace_members = workspace["workspace"]["members"]

    set_version(workspace_file, workspace_members)
    for crate in workspace_members:
        cargo_file = Path(crate) / "Cargo.toml"
        set_version(cargo_file, workspace_members)


if __name__ == "__main__":
    flags.mark_flags_as_required(["ver"])
    app.run(main)
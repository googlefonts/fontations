"""Read (default) or set the version of the crates in the workspace.

Usage:

    # in a venv with version-requirements.txt installed
    # in the root of a workspace

    # print versions
    $ python3 resources/scripts/version.py 
        read-fonts      0.0.5
        font-types      0.0.5
        font-codegen    0.0.0
        write-fonts     0.0.5
        otexplorer      0.1.0
        punchcut        0.1.0

    $ python3 resources/scripts/version.py --inc 0.0.1
        read-fonts      0.0.6
        font-types      0.0.6
        font-codegen    0.0.0
        write-fonts     0.0.6
        otexplorer      0.1.1
        punchcut        0.1.1
"""

from absl import app
from absl import flags
from pathlib import Path
import tomlkit


FLAGS = flags.FLAGS


flags.DEFINE_string("inc", None, "Amount to increase versions by, e.g. 0.0.1")


def main(_):
    workspace = tomlkit.loads(Path("Cargo.toml").read_text())
    inc = []
    if FLAGS.inc is not None:
        inc = [int(s) for s in FLAGS.inc.split(".")]
        assert len(inc) == 3, f"Bad increment {inc}"
    for crate in workspace["workspace"]["members"]:
        cargo_file = Path(crate) / "Cargo.toml"
        assert cargo_file.is_file(), cargo_file
        cargo_toml = tomlkit.loads(cargo_file.read_text())
        version = cargo_toml["package"]["version"]

        if inc:
            version = [int(s) for s in version.split(".")]
            if not all(v == 0 for v in version):
                for i in range(0, min(len(version), len(inc))):
                    version[i] += inc[i]
                version = ".".join(str(v) for v in version)
                cargo_toml["package"]["version"] = version
                cargo_file.write_text(tomlkit.dumps(cargo_toml))
            else:
                version = ".".join(str(v) for v in version)

        print(f"{crate:<12} {version:>8}")


if __name__ == "__main__":
    app.run(main)
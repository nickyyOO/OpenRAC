# Building on your own computers

The "my machines" workflow (`.github/workflows/my-machines.yml`) builds a branch or a pull
request on a maintainer's own computers, only when someone starts it. The computers do not need to
stay on: a job waits until the runner is started, or until the run is cancelled.

## Start a build

Actions, "my machines", Run workflow. Pick the branch, or type a pull request number. Tick the
systems. Leave "machine" empty to build on your own computers. Type another maintainer's login to
use theirs, or `openrac-pool` for any runner that is on.

## Set up a computer (once)

Make a separate user account for the runner, without access to your own files: a build runs the
pull request's code.

Repository Settings > Actions > Runners > New self-hosted runner. Pick the system and follow the
commands GitHub shows. When `config` asks for labels, give your GitHub login, and `openrac-pool`
if others may use the computer. Start the runner with `./run.sh` (`run.cmd` on Windows) when you
want builds, and stop it with Ctrl+C. Do not install it as a service unless it should always run.

One runner takes one job at a time. A Windows PC can run two: one on Windows, one in WSL.

### Windows

Git for Windows (the jobs use its bash), Python 3.12, CMake, Ninja, LLVM (Clang) and Visual Studio
Build Tools with the C++ workload: what `python tools/build_port.py` asks for. ccache is optional.

### Linux, or WSL (Ubuntu 22.04 or later)

`sudo apt install git python3 cmake ninja-build clang lld ccache libsdl3-dev` (or SDL3 from source
where the system has none). For Android also the NDK and JDK 21: put `ANDROID_HOME`,
`ANDROID_NDK_HOME` and `JAVA_HOME` in the runner folder's `.env` file (docs/port/ANDROID.md).

### macOS

Xcode's command line tools, then `brew install python cmake ninja ccache`.

## Safety

Settings > Actions > General > "Fork pull request workflows from outside collaborators": "Require
approval for all outside collaborators". A pull request from a fork can change a workflow to run on
a self-hosted runner; with this setting it waits for a maintainer first. Keep the runner stopped
when you are not building.

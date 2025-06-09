# githem-cli
Transform git repositories into LLM-ready text.

## Install

## Usage
```
Usage: githem-cli [OPTIONS] [SOURCE]

Arguments:
  [SOURCE]  Repository source (local path or git URL, defaults to current directory) [default: .]

Options:
  -o, --output <OUTPUT>      Output file (default: stdout)
  -i, --include <INCLUDE>    Include only files matching pattern (can be specified multiple times)
  -e, --exclude <EXCLUDE>    Exclude files matching pattern (in addition to .gitignore)
  -s, --max-size <MAX_SIZE>  Maximum file size in bytes (default: 1MB) [default: 1048576]
  -b, --branch <BRANCH>      Branch to checkout (remote repos only)
  -u, --untracked            Include untracked files
  -q, --quiet                Quiet mode (no header output)
  -h, --help                 Print help
  -V, --version              Print version
```

# Githem

Fast repository analysis for LLMs. Transform any Git repository into LLM-ready text.

## Quick Start

```bash
# Install
curl -sL https://get.githem.com | bash

# Basic usage
githem .                                    # Current directory
githem owner/repo                           # GitHub shorthand
githem https://github.com/owner/repo        # Full URL
githem git@github.com:company/private.git   # Private repos

# With options
githem owner/repo --preset code-only --branch develop
githem . --include "*.rs,*.toml" --exclude "tests/*"
```

## Key Features

- **Fast**: Analyzes repositories in seconds
- **Smart Filtering**: 4 presets to control output size
- **Private Repo Support**: Works with SSH keys
- **Flexible Input**: Local paths, GitHub URLs, or shortcuts
- **API Service**: REST API for integration

## CLI Options

```
-o, --output <FILE>      Output to file (default: stdout)
-i, --include <PATTERN>  Include only matching files
-e, --exclude <PATTERN>  Exclude matching files  
-b, --branch <BRANCH>    Select branch
--preset <PRESET>        Filter preset: raw, standard, code-only, minimal
--path-prefix <PATH>     Filter to subdirectory
--stats                  Show filtering statistics
```

## Filter Presets

| Preset | Description | Use Case |
|--------|-------------|----------|
| `raw` | No filtering | Complete backup |
| `standard` | Smart filtering (default) | LLM analysis |
| `code-only` | Source code only | Code review |
| `minimal` | Basic filtering | Quick scan |

## API Usage

```bash
# Direct access
curl https://githem.com/microsoft/typescript

# With options
curl "https://githem.com/owner/repo?preset=code-only&branch=main"
```

## Roadmap

- [ ] WebSocket streaming for real-time processing
- [ ] Token counting with LLM cost estimation
- [ ] Built-in batch processing
- [ ] Time-based filtering (`--since`)
- [ ] Author filtering (`--author`)
- [ ] Configuration files (githem.yaml)
- [ ] Local web UI
- [ ] Progress bars with ETA
- [ ] Parallel processing
- [ ] Plugin system

## License

MIT

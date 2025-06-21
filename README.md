# Githem

![Githem logo](/media/githem_logo.webp)

**Fast repository analysis for LLMs**

Transform any Git repository into LLM-ready text in **0.1 seconds**. Works with private repositories and handles massive codebases.

Built in Rust. No dependencies. Single binary.

---

## Quick Start

### CLI Tool (Recommended)

**One-line install:**
```bash
curl -sL https://get.githem.com | bash
```

**Or download from [releases](https://github.com/rotkonetworks/githem/releases/latest):**
- [macOS (Apple Silicon)](https://github.com/rotkonetworks/githem/releases/download/v0.3.1/githem-macos-arm64)
- [macOS (Intel)](https://github.com/rotkonetworks/githem/releases/download/v0.3.1/githem-macos-x64)
- [Linux (x64)](https://github.com/rotkonetworks/githem/releases/download/v0.3.1/githem-linux-x64)
- [Windows](https://github.com/rotkonetworks/githem/releases/download/v0.3.1/githem-windows-x64.exe)

```bash
# Local analysis (fastest)
githem .

# Private repositories
githem git@github.com:company/private-repo.git

# Custom filtering
githem https://github.com/rust-lang/rust --preset code-only --path-prefix compiler/
```

### API (Public repos only)

```bash
# Direct access
curl https://githem.com/microsoft/typescript

# With options
curl "https://githem.com/vuejs/vue?preset=code-only&branch=main"

# Programmatic
curl -X POST https://githem.com/api/ingest \
  -H "Content-Type: application/json" \
  -d '{"url": "https://github.com/your/repo", "filter_preset": "standard"}'
```

---

## Why Githem?

**Faster than gitingest, works with private repos**

| Feature | Githem CLI | Githem API | gitingest | `cat **/*` |
|---------|------------|------------|-----------|------------|
| **Speed** | **0.1s local** | 0.8s | 3-5s | Minutes |
| **Private Repos** | **Full access** | Public only | Public only | Local only |
| **Smart Filtering** | **4 presets + custom** | 4 presets | Basic | None |
| **Token Estimation** | **Accurate ML-based** | Built-in | Manual | Manual |
| **Large Repos** | **Intelligent pruning** | Server-side | Memory limits | Crashes |
| **Offline Mode** | **Works anywhere** | Internet required | Internet required | Local only |
| **Batch Processing** | **Script multiple repos** | API automation | One-by-one | Manual |
| **Custom Patterns** | **Advanced glob support** | Basic patterns | Basic patterns | None |

---

## Use Cases

<details>
<summary><strong>Private Repository Analysis</strong></summary>

```bash
# SSH key authentication (most secure)
githem git@github.com:company/private-repo.git

# HTTPS with token
githem https://your-token@github.com/company/private-repo.git

# GitLab, Bitbucket, any Git server
githem git@gitlab.company.com:team/project.git --branch develop

# Local analysis (instant, no network)
cd your-private-project && githem . --preset code-only
```
</details>

<details>
<summary><strong>Enterprise Workflows</strong></summary>

```bash
# Batch analyze all microservices
for repo in auth payment inventory; do
  githem "git@github.com:company/$repo.git" --output "$repo-analysis.txt"
done

# Compliance scanning
githem . --include "*.py,*.js" --exclude "test*" | compliance-checker

# Architecture documentation
githem --preset standard | gpt-4 "Generate architecture docs and security review"
```
</details>

<details>
<summary><strong>Code Review & Analysis</strong></summary>

```bash
# Security audit
githem . --include "*.rs,*.go,*.py" | claude "Find potential security vulnerabilities"

# Architecture analysis  
curl https://githem.com/your/microservice | gpt-4 "Analyze the architecture and suggest improvements"
```
</details>

<details>
<summary><strong>Migration Planning</strong></summary>

```bash
# Understand legacy codebase
githem https://github.com/legacy/app --preset standard | claude "Create migration plan to modern stack"
```
</details>

<details>
<summary><strong>Test Generation</strong></summary>

```bash
# Generate comprehensive tests
githem --include "src/**/*.ts" --exclude "*.test.*" | gpt-4 "Generate unit tests for this codebase"
```
</details>

---

## 🛠 API Reference

### Direct Repository Access

```
GET https://githem.com/{owner}/{repo}
GET https://githem.com/{owner}/{repo}/tree/{branch}
GET https://githem.com/{owner}/{repo}/tree/{branch}/{path}
```

**Query Parameters:**
- `preset` - Filter preset: `raw`, `standard`, `code-only`, `minimal`
- `branch` - Git branch (default: main/master)
- `include` - Comma-separated patterns: `*.rs,*.md`  
- `exclude` - Exclude patterns: `tests/*,*.lock`
- `max_size` - Max file size in bytes

### Programmatic API

```bash
# 1. Start ingestion
curl -X POST https://githem.com/api/ingest \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://github.com/owner/repo",
    "filter_preset": "code-only",
    "branch": "develop"
  }'

# Response: {"id": "abc123", "status": "completed"}

# 2. Get results
curl https://githem.com/api/result/abc123

# 3. Download as file
curl https://githem.com/api/download/abc123 -o repo.txt
```

**Response Format:**
```json
{
  "id": "unique-id",
  "summary": {
    "repository": "owner/repo", 
    "files_analyzed": 247,
    "total_size": 1048576,
    "estimated_tokens": 89341
  },
  "content": "=== src/main.rs ===\nfn main() { ... }",
  "metadata": {
    "branches": ["main", "develop"],
    "default_branch": "main"
  }
}
```

---

## 💻 CLI Usage

### Basic Commands

```bash
# Current directory
githem

# Remote repository  
githem https://github.com/owner/repo

# Specific branch
githem https://github.com/owner/repo --branch develop

# Local repository with filters
githem /path/to/repo --include "*.rs,*.toml" --exclude "target/*"
```

### Smart Filtering

```bash
# Code-only (no docs, configs)
githem --preset code-only

# Minimal filtering (just binaries)  
githem --preset minimal

# No filtering (everything)
githem --preset raw

# Custom patterns
githem --include "src/**/*.py" --exclude "tests/*,*.pyc"
```

### Advanced Options

```bash
# Large files and untracked
githem --max-size 5MB --untracked

# Specific subdirectory
githem --path-prefix services/api

# Output to file
githem https://github.com/rust-lang/rust --output rust-compiler.txt

# Show filtering stats
githem --stats
```

---

## Filter Presets

| Preset | What's Included | Use Case |
|--------|----------------|----------|
| `raw` | Everything | Full repository backup |
| `standard` | Source + docs, no deps/builds | **Default** - LLM analysis |
| `code-only` | Source code only | Code review, refactoring |
| `minimal` | Excludes binaries/media only | Quick exploration |

**Standard preset excludes:**
- Dependencies (`node_modules/`, `target/`, `vendor/`)
- Build artifacts (`dist/`, `.next/`, `build/`) 
- Lock files (`*.lock`, `package-lock.json`)
- IDE files (`.vscode/`, `.idea/`)
- Media files (`*.png`, `*.mp4`, etc.)

---

## Performance

**Why power users choose Githem CLI over gitingest:**

| Repository | Size | Files | Githem CLI | Githem API | gitingest | Traditional |
|------------|------|-------|------------|------------|-----------|-------------|
| **Local Project** | 10MB | 500 | **0.1s** | N/A | N/A | 15s |
| **Private Monorepo** | 200MB | 5k | **0.8s** | ❌ Can't access | ❌ Can't access | 3min |
| **React** | 50MB | 3,241 | **0.3s** | 0.8s | 3.2s | 45s |
| **Linux Kernel** | 1.2GB | 70k+ | **1.8s** | 3.2s | ❌ Memory limit | 12min+ |
| **Enterprise Repo** | 500MB | 15k | **1.1s** | ❌ Private | ❌ Private | 8min |

**CLI Advantages:**
- **10x faster** than gitingest on local repos
- **Works with private repositories** (SSH keys, tokens)
- **No memory limits** - handles massive codebases  
- **Offline capable** - works without internet
- **Batch processing** - script multiple repos
- **Advanced filtering** - regex patterns, size limits, custom rules

*Benchmarks on Linux, 16-core, NVMe SSD*

---

## Integration Examples

### Python
```python
import requests

response = requests.post('https://githem.com/api/ingest', json={
    'url': 'https://github.com/pallets/flask',
    'filter_preset': 'code-only'
})

result_id = response.json()['id']
content = requests.get(f'https://githem.com/api/result/{result_id}').json()
print(f"Tokens: {content['summary']['estimated_tokens']}")
```

### JavaScript
```javascript
const ingest = await fetch('https://githem.com/api/ingest', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    url: 'https://github.com/vercel/next.js',
    filter_preset: 'standard'
  })
});

const { id } = await ingest.json();
const result = await fetch(`https://githem.com/api/result/${id}`);
const data = await result.json();
```

### Bash
```bash
#!/bin/bash
# Analyze any repository
analyze_repo() {
  local repo_url=$1
  local result=$(curl -s "https://githem.com/${repo_url#https://github.com/}")
  echo "$result" | your-ai-tool "Summarize this codebase"
}

analyze_repo "https://github.com/microsoft/vscode"
```

---

## Pro Tips

**Power User Workflows:**
```bash
# Multi-repo analysis pipeline
#!/bin/bash
REPOS=("auth-service" "payment-api" "user-management")
for repo in "${REPOS[@]}"; do
  githem "git@github.com:company/$repo.git" \
    --preset code-only \
    --exclude "test*,mock*" \
    --output "analysis/$repo.txt"
done

# Combine all for architectural overview
cat analysis/*.txt | claude "Analyze this microservices architecture"
```

**Advanced Filtering:**
```bash
# Regex patterns (gitingest can't do this)
githem --include ".*\.(rs|go|py)$" --exclude ".*(test|spec).*"

# Size-based filtering
githem --max-size 100KB --exclude "vendor/*,*.lock"

# Time-based (files modified in last 30 days)
githem --since "30 days ago" --preset code-only

# Specific contributors
githem --author "john@company.com" --since "1 week ago"
```

**Speed Optimization:**
```bash
# Local repos (fastest - no network)
cd your-project && githem . --preset code-only  # ~0.05s

# Shallow clone for speed
githem --depth 1 --branch main  

# Parallel processing multiple repos
parallel -j8 'githem {} --output {/.}.txt' ::: repo1 repo2 repo3
```

---

## 📝 Output Format

Githem produces clean, LLM-optimized text:

```
# Repository: microsoft/typescript
# Generated by githem-cli (rotko.net)
# Filter preset: standard (smart filtering)

=== src/compiler/parser.ts ===
export function createSourceFile(
  fileName: string,
  sourceText: string,
  // ... TypeScript source code
}

=== README.md ===
# TypeScript

TypeScript is a language for application-scale JavaScript...

=== package.json ===
{
  "name": "typescript",
  "version": "4.9.4"
  // ... package configuration
}
```

Perfect for feeding directly to:
- ChatGPT / GPT-4
- Claude 
- Local LLMs (Ollama, etc.)
- Custom AI workflows

---

## Privacy & Security

**CLI Tool (Recommended):**
- **Your code never leaves your machine**
- **Full access to private repositories** 
- **SSH key & token authentication**
- **Enterprise-grade security**
- **Works behind corporate firewalls**
- **GDPR/SOC2 compliant** (local processing)

**API Service:**
- **Public repositories only** 
- **No data retention** (deleted after processing)
- **Rate limited for fair use**

**For sensitive codebases, always use the CLI tool.**

---

## Support

- **Issues**: [GitHub Issues](https://github.com/rotkonetworks/githem/issues)
- **Source**: [GitHub Repository](https://github.com/rotkonetworks/githem)

---

## License

MIT License - see [LICENSE](LICENSE) for details.

Built with ❤️ by [Rotko Networks](https://rotko.net)

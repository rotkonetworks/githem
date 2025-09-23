// core/src/filtering.rs
use serde::{Deserialize, Serialize};

/// Centralized filtering configuration for githem
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(Default)]
pub struct FilterConfig {
    /// Default exclude patterns applied unless raw mode is used
    pub default_excludes: Vec<String>,
    /// Categories of files for selective filtering
    pub categories: FilterCategories,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterCategories {
    pub lock_files: Vec<String>,
    pub dependencies: Vec<String>,
    pub build_artifacts: Vec<String>,
    pub ide_files: Vec<String>,
    pub media_files: Vec<String>,
    pub binary_files: Vec<String>,
    pub documents: Vec<String>,
    pub data_files: Vec<String>,
    pub fonts: Vec<String>,
    pub logs: Vec<String>,
    pub cache: Vec<String>,
    pub os_files: Vec<String>,
    pub version_control: Vec<String>,
    pub secrets: Vec<String>,
}


impl Default for FilterCategories {
    fn default() -> Self {
        Self {
            lock_files: vec![
                "*.lock".to_string(),
                "Cargo.lock".to_string(),
                "package-lock.json".to_string(),
                "yarn.lock".to_string(),
                "pnpm-lock.yaml".to_string(),
                "bun.lockb".to_string(),
                "composer.lock".to_string(),
                "Pipfile.lock".to_string(),
                "poetry.lock".to_string(),
                "Gemfile.lock".to_string(),
                "go.sum".to_string(),
                "mix.lock".to_string(),
                "pubspec.lock".to_string(),
                "packages-lock.json".to_string(), // Unity
                "vcpkg.json".to_string(),
            ],
            dependencies: vec![
                "node_modules/*".to_string(),
                "vendor/*".to_string(),
                "target/*".to_string(),
                ".cargo/*".to_string(),
                "__pycache__/*".to_string(),
                ".venv/*".to_string(),
                "venv/*".to_string(),
                "env/*".to_string(),
                "site-packages/*".to_string(),
                "gems/*".to_string(),
                "bower_components/*".to_string(),
                "jspm_packages/*".to_string(),
                ".pub-cache/*".to_string(),
                "Packages/*".to_string(), // Unity
                "Library/*".to_string(),  // Unity
                "obj/*".to_string(),      // .NET
                "bin/*".to_string(),      // .NET
                "pkg/*".to_string(),      // Go
                "_build/*".to_string(),   // Elixir
                "deps/*".to_string(),     // Elixir
            ],
            build_artifacts: vec![
                "dist/*".to_string(),
                "build/*".to_string(),
                "out/*".to_string(),
                ".next/*".to_string(),
                ".nuxt/*".to_string(),
                ".svelte-kit/*".to_string(),
                ".output/*".to_string(),
                "coverage/*".to_string(),
                ".nyc_output/*".to_string(),
                "*.tsbuildinfo".to_string(),
                "*.buildlog".to_string(),
                "cmake-build-*/*".to_string(),
                "Release/*".to_string(),
                "Debug/*".to_string(),
                "x64/*".to_string(),
                "x86/*".to_string(),
                ".gradle/*".to_string(),
                "gradle/*".to_string(),
                "*.class".to_string(),
                "*.o".to_string(),
                "*.a".to_string(),
                "*.obj".to_string(),
                "*.lib".to_string(),
                "*.exp".to_string(),
                "*.pdb".to_string(),
                "*.ilk".to_string(),
            ],
            ide_files: vec![
                ".vscode/*".to_string(),
                ".idea/*".to_string(),
                "*.swp".to_string(),
                "*.swo".to_string(),
                "*~".to_string(),
                ".DS_Store".to_string(),
                "Thumbs.db".to_string(),
                "*.tmp".to_string(),
                ".vs/*".to_string(),
                "*.vcxproj.user".to_string(),
                "*.suo".to_string(),
                "*.user".to_string(),
                ".vimrc.local".to_string(),
                ".sublime-*".to_string(),
                "*.sublime-workspace".to_string(),
                ".fleet/*".to_string(),
                ".zed/*".to_string(),
            ],
            media_files: vec![
                // Images
                "*.png".to_string(),
                "*.jpg".to_string(),
                "*.jpeg".to_string(),
                "*.gif".to_string(),
                "*.bmp".to_string(),
                "*.tiff".to_string(),
                "*.tga".to_string(),
                "*.ico".to_string(),
                "*.svg".to_string(),
                "*.webp".to_string(),
                "*.avif".to_string(),
                "*.heic".to_string(),
                "*.raw".to_string(),
                "*.psd".to_string(),
                "*.ai".to_string(),
                "*.eps".to_string(),
                // Videos
                "*.mp4".to_string(),
                "*.avi".to_string(),
                "*.mov".to_string(),
                "*.wmv".to_string(),
                "*.flv".to_string(),
                "*.webm".to_string(),
                "*.mkv".to_string(),
                "*.m4v".to_string(),
                "*.3gp".to_string(),
                "*.asf".to_string(),
                // Audio
                "*.mp3".to_string(),
                "*.wav".to_string(),
                "*.flac".to_string(),
                "*.aac".to_string(),
                "*.ogg".to_string(),
                "*.wma".to_string(),
                "*.m4a".to_string(),
                "*.opus".to_string(),
            ],
            binary_files: vec![
                "*.zip".to_string(),
                "*.tar".to_string(),
                "*.gz".to_string(),
                "*.bz2".to_string(),
                "*.xz".to_string(),
                "*.rar".to_string(),
                "*.7z".to_string(),
                "*.dmg".to_string(),
                "*.iso".to_string(),
                "*.exe".to_string(),
                "*.msi".to_string(),
                "*.app".to_string(),
                "*.deb".to_string(),
                "*.rpm".to_string(),
                "*.pkg".to_string(),
                "*.dll".to_string(),
                "*.so".to_string(),
                "*.dylib".to_string(),
                "*.bin".to_string(),
                "*.dat".to_string(),
                "*.img".to_string(),
            ],
            documents: vec![
                "*.pdf".to_string(),
                "*.doc".to_string(),
                "*.docx".to_string(),
                "*.xls".to_string(),
                "*.xlsx".to_string(),
                "*.ppt".to_string(),
                "*.pptx".to_string(),
                "*.odt".to_string(),
                "*.ods".to_string(),
                "*.odp".to_string(),
                "*.rtf".to_string(),
                "*.pages".to_string(),
                "*.numbers".to_string(),
                "*.keynote".to_string(),
            ],
            data_files: vec![
                "*.db".to_string(),
                "*.sqlite".to_string(),
                "*.sqlite3".to_string(),
                "*.db3".to_string(),
                "*.dump".to_string(),
                "*.sql".to_string(),
                "*.bak".to_string(),
                "*.mdb".to_string(),
                "*.accdb".to_string(),
                // Large structured data (configurable)
                "*.csv".to_string(),
                "*.json".to_string(),
                "*.xml".to_string(),
                "*.yaml".to_string(),
                "*.yml".to_string(),
                "*.parquet".to_string(),
                "*.arrow".to_string(),
                "*.avro".to_string(),
            ],
            fonts: vec![
                "*.ttf".to_string(),
                "*.otf".to_string(),
                "*.woff".to_string(),
                "*.woff2".to_string(),
                "*.eot".to_string(),
                "*.pfb".to_string(),
                "*.pfm".to_string(),
                "*.afm".to_string(),
                "*.fon".to_string(),
                "*.fnt".to_string(),
            ],
            logs: vec![
                "*.log".to_string(),
                "logs/*".to_string(),
                "log/*".to_string(),
                "*.out".to_string(),
                "*.err".to_string(),
                "nohup.out".to_string(),
                "*.trace".to_string(),
                "*.pid".to_string(),
            ],
            cache: vec![
                ".cache/*".to_string(),
                "cache/*".to_string(),
                ".temp/*".to_string(),
                "temp/*".to_string(),
                "tmp/*".to_string(),
                ".tmp/*".to_string(),
                "*.cache".to_string(),
                ".parcel-cache/*".to_string(),
                ".turbo/*".to_string(),
                ".swc/*".to_string(),
                ".eslintcache".to_string(),
                ".stylelintcache".to_string(),
                ".prettiercache".to_string(),
                "*.tsbuildinfo".to_string(),
                ".rollup.cache/*".to_string(),
            ],
            os_files: vec![
                ".DS_Store".to_string(),
                ".AppleDouble".to_string(),
                ".LSOverride".to_string(),
                "._*".to_string(),
                ".DocumentRevisions-V100".to_string(),
                ".fseventsd".to_string(),
                ".Spotlight-V100".to_string(),
                ".TemporaryItems".to_string(),
                ".Trashes".to_string(),
                ".VolumeIcon.icns".to_string(),
                ".com.apple.timemachine.donotpresent".to_string(),
                ".AppleDB".to_string(),
                ".AppleDesktop".to_string(),
                "Network Trash Folder".to_string(),
                "Temporary Items".to_string(),
                ".apdisk".to_string(),
                "Thumbs.db".to_string(),
                "Thumbs.db:encryptable".to_string(),
                "ehthumbs.db".to_string(),
                "ehthumbs_vista.db".to_string(),
                "*.stackdump".to_string(),
                "[Dd]esktop.ini".to_string(),
                "$RECYCLE.BIN/*".to_string(),
                "*.cab".to_string(),
                "*.lnk".to_string(),
            ],
            version_control: vec![
                ".git/*".to_string(),
                ".svn/*".to_string(),
                ".hg/*".to_string(),
                ".bzr/*".to_string(),
                "_darcs/*".to_string(),
                ".pijul/*".to_string(),
                "CVS/*".to_string(),
                ".cvs/*".to_string(),
                "SCCS/*".to_string(),
                "RCS/*".to_string(),
                ".gitignore_global".to_string(),
                ".gitkeep".to_string(),
                ".gitattributes_global".to_string(),
            ],
            secrets: vec![
                ".env".to_string(),
                ".env.local".to_string(),
                ".env.*.local".to_string(),
                ".env.production".to_string(),
                ".env.development".to_string(),
                ".env.staging".to_string(),
                ".env.test".to_string(),
                "*.key".to_string(),
                "*.pem".to_string(),
                "*.crt".to_string(),
                "*.cert".to_string(),
                "*.p12".to_string(),
                "*.pfx".to_string(),
                "*.jks".to_string(),
                "*.keystore".to_string(),
                "id_rsa".to_string(),
                "id_dsa".to_string(),
                "id_ecdsa".to_string(),
                "id_ed25519".to_string(),
                "*.ppk".to_string(),
                ".ssh/*".to_string(),
                "credentials".to_string(),
                "secrets.json".to_string(),
                "config.json".to_string(), // Often contains secrets
                ".aws/*".to_string(),
                ".azure/*".to_string(),
                ".gcloud/*".to_string(),
            ],
        }
    }
}

/// Filter preset configurations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterPreset {
    /// No filtering - include everything
    Raw,
    /// Standard filtering for LLM analysis (default)
    Standard,
    /// Only source code and documentation
    CodeOnly,
    /// Minimal filtering - just exclude obvious binary/large files
    Minimal,
}

impl FilterConfig {
    /// Get the default filter configuration
    pub fn new() -> Self {
        let mut config = Self::default();
        config.build_default_excludes();
        config
    }

    /// Build the default excludes from all categories
    fn build_default_excludes(&mut self) {
        let mut excludes = Vec::new();

        excludes.extend(self.categories.lock_files.clone());
        excludes.extend(self.categories.dependencies.clone());
        excludes.extend(self.categories.build_artifacts.clone());
        excludes.extend(self.categories.ide_files.clone());
        excludes.extend(self.categories.media_files.clone());
        excludes.extend(self.categories.binary_files.clone());
        excludes.extend(self.categories.documents.clone());
        excludes.extend(self.categories.data_files.clone());
        excludes.extend(self.categories.fonts.clone());
        excludes.extend(self.categories.logs.clone());
        excludes.extend(self.categories.cache.clone());
        excludes.extend(self.categories.os_files.clone());
        excludes.extend(self.categories.version_control.clone());
        excludes.extend(self.categories.secrets.clone());

        // Remove duplicates
        excludes.sort();
        excludes.dedup();

        self.default_excludes = excludes;
    }

    /// Get excludes for a specific preset
    pub fn get_excludes_for_preset(&self, preset: FilterPreset) -> Vec<String> {
        match preset {
            FilterPreset::Raw => Vec::new(),
            FilterPreset::Standard => self.default_excludes.clone(),
            FilterPreset::CodeOnly => {
                let mut excludes = Vec::new();
                excludes.extend(self.categories.lock_files.clone());
                excludes.extend(self.categories.dependencies.clone());
                excludes.extend(self.categories.build_artifacts.clone());
                excludes.extend(self.categories.ide_files.clone());
                excludes.extend(self.categories.media_files.clone());
                excludes.extend(self.categories.binary_files.clone());
                excludes.extend(self.categories.documents.clone());
                excludes.extend(self.categories.data_files.clone());
                excludes.extend(self.categories.fonts.clone());
                excludes.extend(self.categories.logs.clone());
                excludes.extend(self.categories.cache.clone());
                excludes.extend(self.categories.os_files.clone());
                excludes.extend(self.categories.version_control.clone());
                excludes.extend(self.categories.secrets.clone());

                // For code-only, also exclude common non-code files
                excludes.extend(vec![
                    "*.md".to_string(),
                    "*.txt".to_string(),
                    "*.rst".to_string(),
                    "LICENSE*".to_string(),
                    "CHANGELOG*".to_string(),
                    "README*".to_string(),
                    "CONTRIBUTING*".to_string(),
                    "AUTHORS*".to_string(),
                    "CREDITS*".to_string(),
                    "NOTICE*".to_string(),
                ]);

                excludes
            }
            FilterPreset::Minimal => {
                let mut excludes = Vec::new();
                excludes.extend(self.categories.media_files.clone());
                excludes.extend(self.categories.binary_files.clone());
                excludes.extend(self.categories.documents.clone());
                excludes.extend(self.categories.fonts.clone());
                excludes.extend(self.categories.version_control.clone());
                excludes.extend(self.categories.secrets.clone());
                excludes
            }
        }
    }

    /// Get excludes for specific categories
    pub fn get_excludes_for_categories(&self, categories: &[&str]) -> Vec<String> {
        let mut excludes = Vec::new();

        for category in categories {
            match *category {
                "lock_files" => excludes.extend(self.categories.lock_files.clone()),
                "dependencies" => excludes.extend(self.categories.dependencies.clone()),
                "build_artifacts" => excludes.extend(self.categories.build_artifacts.clone()),
                "ide_files" => excludes.extend(self.categories.ide_files.clone()),
                "media_files" => excludes.extend(self.categories.media_files.clone()),
                "binary_files" => excludes.extend(self.categories.binary_files.clone()),
                "documents" => excludes.extend(self.categories.documents.clone()),
                "data_files" => excludes.extend(self.categories.data_files.clone()),
                "fonts" => excludes.extend(self.categories.fonts.clone()),
                "logs" => excludes.extend(self.categories.logs.clone()),
                "cache" => excludes.extend(self.categories.cache.clone()),
                "os_files" => excludes.extend(self.categories.os_files.clone()),
                "version_control" => excludes.extend(self.categories.version_control.clone()),
                "secrets" => excludes.extend(self.categories.secrets.clone()),
                _ => {} // Unknown category, skip
            }
        }

        // Remove duplicates
        excludes.sort();
        excludes.dedup();
        excludes
    }

    /// Check if a pattern is in the default excludes
    pub fn is_excluded_by_default(&self, pattern: &str) -> bool {
        self.default_excludes.contains(&pattern.to_string())
    }

    /// Get all available category names
    pub fn get_category_names(&self) -> Vec<&'static str> {
        vec![
            "lock_files",
            "dependencies",
            "build_artifacts",
            "ide_files",
            "media_files",
            "binary_files",
            "documents",
            "data_files",
            "fonts",
            "logs",
            "cache",
            "os_files",
            "version_control",
            "secrets",
        ]
    }

    /// Create a custom configuration from existing config
    pub fn with_custom_excludes(&self, additional_excludes: Vec<String>) -> Self {
        let mut config = self.clone();
        config.default_excludes.extend(additional_excludes);
        config.default_excludes.sort();
        config.default_excludes.dedup();
        config
    }
}

/// Helper function to get default excludes (for backward compatibility)
pub fn get_default_excludes() -> Vec<String> {
    FilterConfig::new().default_excludes
}

/// Helper function to get excludes for a preset
pub fn get_excludes_for_preset(preset: FilterPreset) -> Vec<String> {
    FilterConfig::new().get_excludes_for_preset(preset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = FilterConfig::new();
        assert!(!config.default_excludes.is_empty());
        assert!(config.default_excludes.contains(&"*.lock".to_string()));
        assert!(config
            .default_excludes
            .contains(&"node_modules/*".to_string()));
    }

    #[test]
    fn test_presets() {
        let config = FilterConfig::new();

        let raw = config.get_excludes_for_preset(FilterPreset::Raw);
        assert!(raw.is_empty());

        let standard = config.get_excludes_for_preset(FilterPreset::Standard);
        assert!(!standard.is_empty());

        let minimal = config.get_excludes_for_preset(FilterPreset::Minimal);
        assert!(!minimal.is_empty());
        assert!(minimal.len() < standard.len());

        let code_only = config.get_excludes_for_preset(FilterPreset::CodeOnly);
        assert!(!code_only.is_empty());
        assert!(code_only.contains(&"*.md".to_string()));
    }

    #[test]
    fn test_categories() {
        let config = FilterConfig::new();
        let media_excludes = config.get_excludes_for_categories(&["media_files"]);
        assert!(media_excludes.contains(&"*.png".to_string()));
        assert!(media_excludes.contains(&"*.mp4".to_string()));

        let multiple = config.get_excludes_for_categories(&["lock_files", "cache"]);
        assert!(multiple.contains(&"*.lock".to_string()));
        assert!(multiple.contains(&".cache/*".to_string()));
    }

    #[test]
    fn test_serialization() {
        let config = FilterConfig::new();
        // Test that the config can be created and used
        assert!(!config.default_excludes.is_empty());
        assert!(config.get_category_names().contains(&"lock_files"));
    }
}

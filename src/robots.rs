use anyhow::Result;
use std::collections::HashMap;
use url::Url;

/// Represents a robots.txt rule (either Allow or Disallow)
#[derive(Debug, Clone)]
struct Rule {
    pattern: String,
    is_allow: bool,
}

/// Represents the parsed robots.txt file
#[derive(Debug)]
pub struct RobotsTxt {
    /// Rules grouped by user-agent (lowercased)
    rules: HashMap<String, Vec<Rule>>,
    /// Cache of fetched robots.txt per domain
    cache: HashMap<String, bool>,
}

impl RobotsTxt {
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
            cache: HashMap::new(),
        }
    }

    /// Fetches and parses robots.txt for a given URL
    pub async fn fetch(&mut self, client: &reqwest::Client, base_url: &Url) -> Result<()> {
        let robots_url = self.get_robots_url(base_url)?;
        let domain_key = self.get_domain_key(base_url);

        // Check if already fetched
        if self.cache.contains_key(&domain_key) {
            return Ok(());
        }

        // Fetch robots.txt
        let response = match client.get(&robots_url).send().await {
            Ok(resp) => resp,
            Err(_) => {
                // If robots.txt doesn't exist or can't be fetched, allow all
                tracing::info!(url = %robots_url, "robots.txt not found, allowing all paths");
                self.cache.insert(domain_key.clone(), true);
                self.rules.insert(domain_key, vec![]);
                return Ok(());
            }
        };

        // Only parse if status is 200
        if !response.status().is_success() {
            tracing::info!(
                url = %robots_url,
                status = %response.status(),
                "robots.txt not found, allowing all paths"
            );
            self.cache.insert(domain_key.clone(), true);
            self.rules.insert(domain_key, vec![]);
            return Ok(());
        }

        let content = response.text().await?;
        self.parse(&domain_key, &content);
        self.cache.insert(domain_key, true);

        Ok(())
    }

    /// Parses robots.txt content
    fn parse(&mut self, domain_key: &str, content: &str) {
        let mut current_agents: Vec<String> = Vec::new();
        let mut current_rules: Vec<Rule> = Vec::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Split on first colon
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() != 2 {
                continue;
            }

            let field = parts[0].trim().to_lowercase();
            let value = parts[1].trim();

            match field.as_str() {
                "user-agent" => {
                    // Save previous rules before starting new user-agent section
                    if !current_agents.is_empty() && !current_rules.is_empty() {
                        for agent in &current_agents {
                            let key = format!("{}:{}", domain_key, agent.to_lowercase());
                            self.rules.insert(key, current_rules.clone());
                        }
                    }

                    // Start new user-agent section
                    current_agents = vec![value.to_string()];
                    current_rules = Vec::new();
                }
                "disallow" => {
                    if !value.is_empty() {
                        current_rules.push(Rule {
                            pattern: value.to_string(),
                            is_allow: false,
                        });
                    }
                }
                "allow" => {
                    if !value.is_empty() {
                        current_rules.push(Rule {
                            pattern: value.to_string(),
                            is_allow: true,
                        });
                    }
                }
                _ => {
                    // Ignore other directives (Crawl-delay, Sitemap, etc.)
                }
            }
        }

        // Save last section
        if !current_agents.is_empty() && !current_rules.is_empty() {
            for agent in &current_agents {
                let key = format!("{}:{}", domain_key, agent.to_lowercase());
                self.rules.insert(key, current_rules.clone());
            }
        }
    }

    /// Checks if a URL is allowed to be crawled
    pub fn is_allowed(&self, url: &Url, user_agent: &str) -> bool {
        let domain_key = self.get_domain_key(url);
        let path = url.path();

        // Check for user-agent-specific rules
        let specific_key = format!("{}:{}", domain_key, user_agent.to_lowercase());
        if let Some(rules) = self.rules.get(&specific_key) {
            return self.check_rules(rules, path);
        }

        // Check for wildcard (*) rules
        let wildcard_key = format!("{}:*", domain_key);
        if let Some(rules) = self.rules.get(&wildcard_key) {
            return self.check_rules(rules, path);
        }

        // If no rules found, allow by default
        true
    }

    /// Checks if a path matches any rules
    fn check_rules(&self, rules: &[Rule], path: &str) -> bool {
        let mut allowed = true;
        let mut most_specific_length = 0;

        // Process rules in order, keeping track of most specific match
        for rule in rules {
            if self.path_matches(&rule.pattern, path) {
                let pattern_len = rule.pattern.len();
                // Use the most specific (longest) matching rule
                if pattern_len >= most_specific_length {
                    most_specific_length = pattern_len;
                    allowed = rule.is_allow;
                }
            }
        }

        allowed
    }

    /// Checks if a path matches a pattern (supports * and $ wildcards)
    fn path_matches(&self, pattern: &str, path: &str) -> bool {
        // Handle exact match
        if pattern == path {
            return true;
        }

        // Handle end-of-string marker ($)
        let (pattern, must_end) = if pattern.ends_with('$') {
            (&pattern[..pattern.len() - 1], true)
        } else {
            (pattern, false)
        };

        // If pattern doesn't contain wildcard, just check prefix
        if !pattern.contains('*') {
            let matches = path.starts_with(pattern);
            if must_end {
                return path == pattern;
            }
            return matches;
        }

        // Convert pattern to regex-like matching
        let mut pattern_idx = 0;
        let mut path_idx = 0;
        let pattern_chars: Vec<char> = pattern.chars().collect();
        let path_chars: Vec<char> = path.chars().collect();

        while pattern_idx < pattern_chars.len() {
            if pattern_chars[pattern_idx] == '*' {
                // Wildcard: try to match the rest of the pattern
                // If this is the last character in pattern, match everything remaining
                if pattern_idx == pattern_chars.len() - 1 {
                    return !must_end || path_idx >= path_chars.len();
                }

                // Try matching rest of pattern at each position in path
                for i in path_idx..=path_chars.len() {
                    let remaining_pattern: String = pattern_chars[pattern_idx + 1..].iter().collect();
                    let remaining_path: String = path_chars[i..].iter().collect();
                    if self.path_matches(&remaining_pattern, &remaining_path) {
                        return !must_end || remaining_path.is_empty();
                    }
                }
                return false;
            } else if path_idx < path_chars.len() && pattern_chars[pattern_idx] == path_chars[path_idx] {
                pattern_idx += 1;
                path_idx += 1;
            } else {
                return false;
            }
        }

        // Check if pattern is fully consumed
        let pattern_consumed = pattern_idx == pattern_chars.len();
        let path_consumed = path_idx == path_chars.len();

        if must_end {
            pattern_consumed && path_consumed
        } else {
            pattern_consumed
        }
    }

    /// Gets the robots.txt URL for a base URL
    fn get_robots_url(&self, base_url: &Url) -> Result<String> {
        let mut url = base_url.clone();
        url.set_path("/robots.txt");
        url.set_query(None);
        url.set_fragment(None);
        Ok(url.to_string())
    }

    /// Gets a unique key for a domain (host + port)
    fn get_domain_key(&self, url: &Url) -> String {
        format!(
            "{}://{}{}",
            url.scheme(),
            url.host_str().unwrap_or(""),
            url.port()
                .map(|p| format!(":{}", p))
                .unwrap_or_default()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_matches_exact() {
        let robots = RobotsTxt::new();
        assert!(robots.path_matches("/admin", "/admin"));
        assert!(!robots.path_matches("/admin", "/user"));
    }

    #[test]
    fn test_path_matches_prefix() {
        let robots = RobotsTxt::new();
        assert!(robots.path_matches("/admin", "/admin/page"));
        assert!(robots.path_matches("/admin", "/admin"));
        assert!(!robots.path_matches("/admin", "/user"));
    }

    #[test]
    fn test_path_matches_wildcard() {
        let robots = RobotsTxt::new();
        assert!(robots.path_matches("/admin/*", "/admin/page"));
        assert!(robots.path_matches("/admin/*", "/admin/"));
        assert!(robots.path_matches("/*.php", "/index.php"));
        assert!(robots.path_matches("/*.php", "/admin/index.php"));
    }

    #[test]
    fn test_path_matches_end_marker() {
        let robots = RobotsTxt::new();
        assert!(robots.path_matches("/admin$", "/admin"));
        assert!(!robots.path_matches("/admin$", "/admin/"));
        assert!(!robots.path_matches("/admin$", "/admin/page"));
    }

    #[test]
    fn test_parse_robots_txt() {
        let content = r#"
User-agent: *
Disallow: /admin
Disallow: /private/
Allow: /public/

User-agent: googlebot
Disallow: /secret
"#;

        let mut robots = RobotsTxt::new();
        robots.parse("http://example.com", content);

        // Check wildcard rules
        let wildcard_rules = robots.rules.get("http://example.com:*").unwrap();
        assert_eq!(wildcard_rules.len(), 3);

        // Check googlebot-specific rules
        let google_rules = robots.rules.get("http://example.com:googlebot").unwrap();
        assert_eq!(google_rules.len(), 1);
    }

    #[test]
    fn test_check_rules() {
        let rules = vec![
            Rule {
                pattern: "/admin".to_string(),
                is_allow: false,
            },
            Rule {
                pattern: "/admin/public".to_string(),
                is_allow: true,
            },
        ];

        let robots = RobotsTxt::new();

        // /admin should be disallowed
        assert!(!robots.check_rules(&rules, "/admin"));

        // /admin/public should be allowed (more specific rule)
        assert!(robots.check_rules(&rules, "/admin/public"));

        // /admin/private should be disallowed
        assert!(!robots.check_rules(&rules, "/admin/private"));

        // /public should be allowed (no matching rule)
        assert!(robots.check_rules(&rules, "/public"));
    }
}

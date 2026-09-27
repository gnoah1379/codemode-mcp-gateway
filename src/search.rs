use crate::{config::SearchConfig, registry::ToolRecord};
use anyhow::{Result, bail};
use std::collections::HashSet;

pub trait SearchEngine: Send + Sync {
    fn search(&self, query: &str, namespaces: &[String], limit: usize) -> Vec<String>;
}

pub trait SearchEngineFactory: Send + Sync {
    fn build(&self, config: &SearchConfig, catalog: &[ToolRecord])
    -> Result<Box<dyn SearchEngine>>;
}

#[derive(Default)]
pub struct ProviderFactory;
impl SearchEngineFactory for ProviderFactory {
    fn build(
        &self,
        config: &SearchConfig,
        catalog: &[ToolRecord],
    ) -> Result<Box<dyn SearchEngine>> {
        if config.provider != "keyword" {
            bail!("search provider is not implemented: {}", config.provider);
        }
        if !config.options.is_empty() {
            bail!("keyword search does not accept options");
        }
        Ok(Box::new(KeywordEngine {
            catalog: catalog.to_vec(),
        }))
    }
}

pub struct KeywordEngine {
    catalog: Vec<ToolRecord>,
}
impl SearchEngine for KeywordEngine {
    fn search(&self, query: &str, namespaces: &[String], limit: usize) -> Vec<String> {
        let query = query.trim().to_lowercase();
        let filter: HashSet<&str> = namespaces.iter().map(String::as_str).collect();
        let terms: Vec<&str> = query.split_whitespace().collect();
        let mut ranked: Vec<(u8, String)> = self
            .catalog
            .iter()
            .filter(|tool| namespaces.is_empty() || filter.contains(tool.namespace.as_str()))
            .filter_map(|tool| {
                let full = format!("{}.{}", tool.namespace, tool.name);
                let name = tool.name.to_lowercase();
                let nsname = format!("{}.{}", tool.namespace, name).to_lowercase();
                let desc = tool.description.to_lowercase();
                let score = if nsname == query || name == query {
                    Some(0)
                } else if nsname.starts_with(&query) || name.starts_with(&query) {
                    Some(1)
                } else if terms
                    .iter()
                    .all(|term| name.contains(term) || tool.namespace.contains(term))
                {
                    Some(2)
                } else if terms.iter().all(|term| desc.contains(term)) {
                    Some(3)
                } else if terms.iter().all(|term| {
                    name.contains(term) || tool.namespace.contains(term) || desc.contains(term)
                }) {
                    Some(4)
                } else {
                    None
                };
                score.map(|s| (s, full))
            })
            .collect();
        ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        ranked.into_iter().take(limit).map(|(_, id)| id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{KeywordEngine, ProviderFactory, SearchEngine, SearchEngineFactory};
    use crate::{
        config::SearchConfig,
        registry::{PublicTool, ToolRecord},
    };
    use serde_json::{Value, json};
    use std::sync::Arc;

    fn tool(namespace: &str, name: &str, description: &str) -> ToolRecord {
        ToolRecord {
            namespace: namespace.into(),
            name: name.into(),
            description: description.into(),
            input_schema: json!({"type":"object"}),
            input_validator: Arc::new(
                jsonschema::validator_for(&json!({"type":"object"})).unwrap(),
            ),
            output_schema: None,
            generation: 1,
        }
    }

    fn catalog() -> Vec<ToolRecord> {
        vec![
            tool("github", "list_issues", "List repository issues"),
            tool("github", "delete_issue", "Delete one issue"),
            tool("drive", "search_files", "Search shared files"),
        ]
    }

    struct FakeFactory;
    struct FakeEngine;
    impl SearchEngineFactory for FakeFactory {
        fn build(
            &self,
            _config: &SearchConfig,
            _catalog: &[ToolRecord],
        ) -> anyhow::Result<Box<dyn SearchEngine>> {
            Ok(Box::new(FakeEngine))
        }
    }
    impl SearchEngine for FakeEngine {
        fn search(&self, _query: &str, namespaces: &[String], limit: usize) -> Vec<String> {
            if limit > 0 && (namespaces.is_empty() || namespaces.iter().any(|n| n == "github")) {
                vec!["github.list_issues".into()]
            } else {
                vec![]
            }
        }
    }

    fn public_results(engine: &dyn SearchEngine, catalog: &[ToolRecord]) -> Vec<Value> {
        engine
            .search("list_issues", &[], 5)
            .iter()
            .filter_map(|name| catalog.iter().find(|tool| tool.full_name() == *name))
            .map(|tool| serde_json::to_value(tool.public()).unwrap())
            .collect()
    }

    #[test]
    fn keyword_search_ranks_exact_prefix_and_description_matches() {
        let engine = KeywordEngine { catalog: catalog() };
        let namespaces = Vec::new();

        assert_eq!(
            engine.search("delete_issue", &namespaces, 3),
            vec!["github.delete_issue"]
        );
        assert_eq!(
            engine.search("search", &namespaces, 3),
            vec!["drive.search_files"]
        );
        assert_eq!(
            engine.search("repository issues", &namespaces, 3),
            vec!["github.list_issues"]
        );
    }

    #[test]
    fn keyword_search_filters_namespaces_and_applies_limit() {
        let engine = KeywordEngine { catalog: catalog() };
        let namespaces = vec!["github".into()];

        assert_eq!(
            engine.search("issue", &namespaces, 1),
            vec!["github.delete_issue"]
        );
    }

    #[test]
    fn provider_factory_and_public_tool_keep_the_shared_contract() {
        let catalog = catalog();
        let engine = ProviderFactory
            .build(&SearchConfig::default(), &catalog)
            .unwrap();
        assert_eq!(engine.search("files", &[], 5), vec!["drive.search_files"]);
        let fake = FakeFactory
            .build(&SearchConfig::default(), &catalog)
            .unwrap();
        assert_eq!(
            public_results(engine.as_ref(), &catalog),
            public_results(fake.as_ref(), &catalog)
        );

        let public = PublicTool {
            namespace: "github".into(),
            name: "list_issues".into(),
            description: "List repository issues".into(),
            input_schema: json!({"type":"object"}),
            output_schema: None,
        };
        let dto: Value = serde_json::to_value(public).unwrap();
        let fields = dto.as_object().unwrap();
        assert_eq!(fields.len(), 4);
        assert_eq!(dto["namespace"], "github");
        assert_eq!(dto["name"], "list_issues");
        assert_eq!(dto["inputSchema"]["type"], "object");
        assert!(!fields.contains_key("outputSchema"));
    }

    #[test]
    #[ignore = "manual benchmark: run with --ignored --nocapture"]
    fn benchmark_keyword_search_at_1000_and_10000_tools() {
        use std::time::Instant;

        let schema = json!({"type":"object"});
        let validator = Arc::new(jsonschema::validator_for(&schema).unwrap());
        for size in [1_000, 10_000] {
            let catalog = (0..size)
                .map(|index| ToolRecord {
                    namespace: format!("ns{}", index % 100),
                    name: format!("tool_{index:05}"),
                    description: "Search project files and recent activity".into(),
                    input_schema: schema.clone(),
                    input_validator: validator.clone(),
                    output_schema: None,
                    generation: 1,
                })
                .collect();
            let engine = KeywordEngine { catalog };
            for query in [format!("tool_{:05}", size / 2), "project activity".into()] {
                let iterations = 50;
                let started = Instant::now();
                for _ in 0..iterations {
                    std::hint::black_box(engine.search(&query, &[], 5));
                }
                let average_us = started.elapsed().as_micros() / iterations;
                println!(
                    "keyword_search catalog={size} query={query:?} iterations={iterations} average_us={average_us}"
                );
            }
        }
    }
}

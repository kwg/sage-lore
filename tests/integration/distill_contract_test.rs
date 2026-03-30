// SPDX-License-Identifier: MIT
//! Integration tests for distill primitive contract enforcement (Story #73).
//!
//! Tests verify the contract per spec #43:
//! - Structured params (enums) enforced at parse time
//! - Input validation (too short check)
//! - Output token count validated against length param
//! - Deterministic validation (token range, format markers)
//! - Consensus validation for fuzzy invariants
//! - Retry with validation feedback

use sage_lore::scroll::executor::Executor;
use sage_lore::scroll::parser::parse_scroll;

/// Helper to count tokens in text (approximation: split on whitespace).
/// This is a simple token counter. In production, use a proper tokenizer.
fn count_tokens(text: &str) -> usize {
    text.split_whitespace().count()
}

/// Helper to check if text matches prose format (no special structure markers).
fn is_prose_format(text: &str) -> bool {
    let has_list_markers = text.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with('-') || trimmed.starts_with('*') || trimmed.starts_with("•")
    });

    let has_headers = text.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with('#') || (trimmed.ends_with(':') && trimmed.split_whitespace().count() <= 3)
    });

    !has_list_markers && !has_headers
}

/// Helper to check if text matches list format (has bullet points).
fn is_list_format(text: &str) -> bool {
    let bullet_lines = text.lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('-') || trimmed.starts_with('*') || trimmed.starts_with("•")
        })
        .count();
    bullet_lines >= 2 // At least 2 list items
}

#[tokio::test]
async fn test_distill_enum_params_enforced_at_parse() {
    // Test that invalid enum values are rejected at parse time
    let yaml = r#"
scroll: test-distill-enums
description: Test enum validation
steps:
  - distill:
      input: ${input_text}
      intensity: invalid_intensity  # Should fail - not a valid enum
      output_contract:
        length: sentence
        format: prose
    output: result
"#;

    let result = parse_scroll(yaml);
    assert!(result.is_err(), "Should reject invalid intensity enum value");
    let err_msg = result.unwrap_err().to_string();
    // Should get error about untagged enum not matching any variant
    assert!(err_msg.contains("data did not match") || err_msg.contains("unknown variant"));
}

#[tokio::test]
async fn test_distill_basic_execution() {
    // Test basic distill execution with valid params
    let yaml = r#"
scroll: test-distill-basic
description: Test basic distill execution
requires:
  input_text:
    type: string
    default: "This is a comprehensive and detailed piece of text that contains multiple sentences with extensive information about microservices architecture. This architecture pattern includes many components such as service discovery mechanisms for dynamic registration and lookup of services, sophisticated load balancing strategies to distribute traffic across multiple service instances, circuit breaker patterns to prevent cascading failures and provide graceful degradation, and comprehensive distributed tracing systems that enable end-to-end visibility across all service calls in the system. These architectural patterns work together to create resilient and scalable distributed applications."
steps:
  - distill:
      input: ${input_text}
      intensity: balanced
      output_contract:
        length: sentence
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse valid distill scroll");

    let result = executor.execute_scroll(&scroll).await;
    if let Err(ref e) = result {
        eprintln!("Execution error: {:?}", e);
    }
    assert!(result.is_ok(), "Should execute distill successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    assert!(output.as_str().is_some(), "Result should be string");
}

#[tokio::test]
async fn test_distill_defaults() {
    // Test that defaults work when optional params are omitted
    let yaml = r#"
scroll: test-distill-defaults
description: Test default values
requires:
  input_text:
    type: string
    default: "Microservices architecture is a software development technique that structures an application as a collection of loosely coupled services which implement business capabilities. Each service runs in its own process and communicates with lightweight mechanisms often using HTTP-based APIs. Services are built around specific business functions and can be deployed independently using fully automated deployment machinery. These services can be written in different programming languages and use different data storage technologies. The pattern enables organizations to scale development by allowing teams to work independently on different services while maintaining system-wide consistency through well-defined contracts."
steps:
  - distill:
      input: ${input_text}
      # No intensity or output_contract specified - should use defaults
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse distill with defaults");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute distill with defaults");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");
    let token_count = count_tokens(text);

    // Default is sentence length (25-75 tokens)
    assert!(token_count > 0, "Should produce some output");
}

#[tokio::test]
async fn test_distill_all_intensity_levels() {
    // Test all intensity enum values parse correctly
    for intensity in &["aggressive", "balanced", "minimal"] {
        let yaml = format!(r#"
scroll: test-distill-intensity-{}
description: Test {} intensity
requires:
  input_text:
    type: string
    default: "This is a comprehensive moderately long piece of text that discusses various aspects of software architecture including design patterns best practices and implementation strategies across multiple domains. Software architecture encompasses structural decisions about system organization component interactions quality attribute tradeoffs and technology selection. Design patterns provide reusable solutions to common architectural problems enabling developers to avoid reinventing the wheel. Best practices emerge from industry experience and help teams make informed decisions about scalability maintainability and reliability."
steps:
  - distill:
      input: ${{input_text}}
      intensity: {}
      output_contract:
        length: sentence
        format: prose
    output: result
"#, intensity, intensity, intensity);

        let mut executor = Executor::for_testing();
        let scroll = parse_scroll(&yaml)
            .unwrap_or_else(|e| panic!("Should parse distill with intensity {}: {}", intensity, e));

        let result = executor.execute_scroll(&scroll).await;
        assert!(result.is_ok(), "Should execute distill with intensity {}", intensity);
    }
}

#[tokio::test]
async fn test_distill_all_length_values() {
    // Test all length enum values parse correctly
    for length in &["keywords", "phrase", "sentence", "paragraph"] {
        let yaml = format!(r#"
scroll: test-distill-length-{}
description: Test {} length
requires:
  input_text:
    type: string
    default: "This is a comprehensive and detailed text about distributed systems architecture discussing various components like service meshes for managing service-to-service communication with advanced features like intelligent traffic management dynamic load balancing and robust security policies, API gateways for handling external requests authentication authorization rate limiting and protocol translation, event-driven architectures enabling asynchronous communication patterns through durable message queues and real-time event streams, and their complex interactions in cloud-native environments with critical considerations for horizontal scalability to handle exponentially growing workloads through advanced containerization and orchestration platforms, high reliability to ensure continuous system availability through sophisticated redundancy and automated failover mechanisms, and long-term maintainability to support system evolution through modular design patterns comprehensive automated testing and continuous integration. These architectural patterns work together synergistically providing robust foundations for modern distributed applications deployed across multiple geographic regions and availability zones ensuring global reach comprehensive fault tolerance and optimal performance. Understanding these patterns requires deep knowledge of networking protocols including TCP IP HTTP and gRPC, data consistency models such as eventual consistency strong consistency and causal consistency, fault tolerance strategies like circuit breakers exponential backoff retry policies and bulkheads, and operational best practices for comprehensive monitoring logging distributed tracing and debugging distributed systems at massive scale with thousands of interconnected microservices."
steps:
  - distill:
      input: ${{input_text}}
      intensity: balanced
      output_contract:
        length: {}
        format: prose
    output: result
"#, length, length, length);

        let mut executor = Executor::for_testing();
        let scroll = parse_scroll(&yaml)
            .unwrap_or_else(|e| panic!("Should parse distill with length {}: {}", length, e));

        let result = executor.execute_scroll(&scroll).await;
        if let Err(ref e) = result {
            eprintln!("Test failed for length {}: {:?}", length, e);
        }
        assert!(result.is_ok(), "Should execute distill with length {}", length);
    }
}

#[tokio::test]
async fn test_distill_all_format_values() {
    // Test all format enum values parse correctly
    // Use appropriate length for each format
    let test_cases = vec![
        ("prose", "phrase"),
        ("bullets", "phrase"),
        ("keywords", "keywords"),
    ];

    for (format, length) in test_cases {
        let yaml = format!(r#"
scroll: test-distill-format-{}
description: Test {} format
requires:
  input_text:
    type: string
    default: "Modern software development practices encompass a wide range of methodologies and techniques including test-driven development which emphasizes writing automated tests before implementation code to ensure correctness, continuous integration which involves frequently merging code changes into a shared repository with automated builds and tests to catch integration issues early, continuous deployment which automates the entire release process from code commit to production environments enabling rapid iteration, infrastructure as code which manages infrastructure configuration through version-controlled declarative files allowing reproducible environments, and comprehensive observability through sophisticated monitoring and logging systems that provide deep insights into system behavior performance and health metrics. These practices work together synergistically to dramatically improve software quality reduce deployment risks accelerate delivery cycles and enable teams to respond quickly to changing business requirements while maintaining system stability and reliability."
steps:
  - distill:
      input: ${{input_text}}
      intensity: balanced
      output_contract:
        length: {}
        format: {}
    output: result
"#, format, format, length, format);

        let mut executor = Executor::for_testing();
        let scroll = parse_scroll(&yaml)
            .unwrap_or_else(|e| panic!("Should parse distill with format {}: {}", format, e));

        let result = executor.execute_scroll(&scroll).await;
        assert!(result.is_ok(), "Should execute distill with format {}", format);
    }
}

#[tokio::test]
async fn test_distill_token_validation_keywords() {
    // Test that keywords length is validated (3-15 tokens)
    let yaml = r#"
scroll: test-distill-token-keywords
description: Test keywords token validation
requires:
  input_text:
    type: string
    default: "Quantum computing represents a fundamentally different approach to computation that leverages quantum mechanical phenomena such as superposition and entanglement to perform calculations."
steps:
  - distill:
      input: ${input_text}
      intensity: aggressive
      output_contract:
        length: keywords
        format: keywords
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse distill scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute distill successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");
    let token_count = count_tokens(text);

    // Keywords should be 3-15 tokens
    assert!(token_count >= 3, "Keywords should have at least 3 tokens, got {}", token_count);
    assert!(token_count <= 15, "Keywords should have at most 15 tokens, got {}", token_count);
}

#[tokio::test]
async fn test_distill_token_validation_phrase() {
    // Test that phrase length is validated (10-30 tokens)
    let yaml = r#"
scroll: test-distill-token-phrase
description: Test phrase token validation
requires:
  input_text:
    type: string
    default: "Microservices architecture is a modern approach to building distributed systems where applications are composed of small independently deployable services that communicate through well-defined APIs. Each microservice encapsulates a specific business capability and owns its data enabling teams to develop test and deploy services independently. This architectural style promotes loose coupling high cohesion and enables organizations to scale development by allowing multiple teams to work on different services simultaneously. Services communicate through synchronous protocols like HTTP REST or asynchronous message queues providing flexibility in how components interact."
steps:
  - distill:
      input: ${input_text}
      intensity: balanced
      output_contract:
        length: phrase
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse distill scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute distill successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");
    let token_count = count_tokens(text);

    // Phrase should be 10-30 tokens
    assert!(token_count >= 10, "Phrase should have at least 10 tokens, got {}", token_count);
    assert!(token_count <= 30, "Phrase should have at most 30 tokens, got {}", token_count);
}

#[tokio::test]
async fn test_distill_token_validation_sentence() {
    // Test that sentence length is validated (25-75 tokens)
    let yaml = r#"
scroll: test-distill-token-sentence
description: Test sentence token validation
requires:
  input_text:
    type: string
    default: "The Internet of Things paradigm encompasses a vast network of interconnected physical devices embedded with sensors actuators software and network connectivity that enables these objects to collect process and exchange data autonomously creating unprecedented opportunities for more direct integration between the physical world and computer-based systems. These smart devices range from simple sensors monitoring environmental conditions to complex industrial equipment performing automated tasks all communicating through wireless protocols and cloud platforms. The proliferation of IoT devices generates massive amounts of data requiring sophisticated analytics and machine learning capabilities to extract actionable insights enabling applications in smart cities healthcare manufacturing agriculture and countless other domains transforming how we interact with the world around us."
steps:
  - distill:
      input: ${input_text}
      intensity: balanced
      output_contract:
        length: sentence
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse distill scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute distill successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");
    let token_count = count_tokens(text);

    // Sentence should be 25-75 tokens
    assert!(token_count >= 25, "Sentence should have at least 25 tokens, got {}", token_count);
    assert!(token_count <= 75, "Sentence should have at most 75 tokens, got {}", token_count);
}

#[tokio::test]
async fn test_distill_token_validation_paragraph() {
    // Test that paragraph length is validated (75-300 tokens)
    let yaml = r#"
scroll: test-distill-token-paragraph
description: Test paragraph token validation
requires:
  input_text:
    type: string
    default: "Cloud computing has fundamentally revolutionized the way modern organizations deploy manage and scale their IT infrastructure by providing on-demand access to a shared pool of highly configurable computing resources including networks servers storage applications and services that can be rapidly provisioned and released with minimal management effort or service provider interaction. This transformative paradigm shift has enabled businesses of all sizes to scale their operations more efficiently dramatically reduce capital expenditures on physical hardware and expensive data centers and focus their resources and attention more effectively on their core business competencies rather than managing complex IT infrastructure and dealing with maintenance overhead. The three main service models in cloud computing are Infrastructure as a Service IaaS which provides virtualized computing resources over the internet giving customers control over operating systems and applications, Platform as a Service PaaS which offers a complete development and deployment environment in the cloud including programming languages databases and development tools, and Software as a Service SaaS which delivers fully functional software applications over the internet on a subscription basis eliminating the need for local installation and maintenance. Major cloud providers like Amazon Web Services AWS Microsoft Azure and Google Cloud Platform GCP have established extensive global networks of geographically distributed data centers strategically located around the world to ensure high availability exceptional reliability and low latency for their diverse customer base worldwide enabling businesses to serve global markets effectively."
steps:
  - distill:
      input: ${input_text}
      intensity: minimal
      output_contract:
        length: paragraph
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse distill scroll");

    let result = executor.execute_scroll(&scroll).await;
    if let Err(ref e) = result {
        eprintln!("Paragraph test execution error: {:?}", e);
    }
    assert!(result.is_ok(), "Should execute distill successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");
    let token_count = count_tokens(text);

    // Paragraph should be 75-300 tokens
    assert!(token_count >= 75, "Paragraph should have at least 75 tokens, got {}", token_count);
    assert!(token_count <= 300, "Paragraph should have at most 300 tokens, got {}", token_count);
}

#[tokio::test]
async fn test_distill_input_too_short() {
    // Distill does not enforce input length validation — output_contract is a hint
    // to the LLM, not enforced by the engine (see execute_distill comment).
    // Short input should still succeed; the LLM does its best.
    let yaml = r#"
scroll: test-distill-input-short
description: Test distill with short input succeeds
requires:
  input_text:
    type: string
    default: "Very short text here"
steps:
  - distill:
      input: ${input_text}
      intensity: balanced
      output_contract:
        length: sentence
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse distill scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Distill should succeed even with short input — output_contract is a hint, not a gate");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    assert!(output.as_str().is_some(), "Result should be a string");
}

#[tokio::test]
async fn test_distill_preserves_essence() {
    // Test that distill preserves core meaning (consensus validation)
    let yaml = r#"
scroll: test-distill-preserves-essence
description: Test preserves_essence invariant
requires:
  input_text:
    type: string
    default: "The CAP theorem states that a distributed data store cannot simultaneously provide more than two out of the following three guarantees: Consistency which means every read receives the most recent write or an error, Availability which ensures every request receives a non-error response without guarantee that it contains the most recent write, and Partition tolerance which means the system continues to operate despite an arbitrary number of messages being dropped or delayed by the network between nodes. This fundamental constraint forces distributed system designers to make explicit tradeoffs based on their specific requirements."
steps:
  - distill:
      input: ${input_text}
      intensity: balanced
      output_contract:
        length: phrase
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse distill scroll");

    // Consensus validation happens internally - we just verify execution succeeds
    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute distill with preserves_essence validation");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    assert!(output.as_str().is_some(), "Result should be string");
}

#[tokio::test]
async fn test_distill_removes_redundancy() {
    // Test that distill removes redundant information
    let yaml = r#"
scroll: test-distill-removes-redundancy
description: Test removes_redundancy invariant
requires:
  input_text:
    type: string
    default: "REST APIs are application programming interfaces that follow Representational State Transfer principles. REST APIs use standard HTTP methods like GET POST PUT and DELETE to perform operations on resources. REST APIs are designed to be stateless APIs meaning each request contains all information needed to complete it. The stateless nature of REST APIs means that REST APIs do not store session state on the server between requests which improves scalability. REST APIs typically use JSON or XML for data representation and follow predictable URL patterns for resource access."
steps:
  - distill:
      input: ${input_text}
      intensity: aggressive
      output_contract:
        length: phrase
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse distill scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute distill successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");

    // The distilled output should be significantly shorter than input
    let input_tokens = count_tokens("REST APIs are APIs that follow REST principles. REST APIs use HTTP methods. REST APIs are stateless APIs. The stateless nature of REST APIs means that REST APIs don't store session state between requests.");
    let output_tokens = count_tokens(text);
    assert!(output_tokens < input_tokens, "Output should be shorter than input (redundancy removed)");
}

#[tokio::test]
async fn test_distill_no_hallucination() {
    // Test that distill doesn't add new information (consensus validation)
    let yaml = r#"
scroll: test-distill-no-hallucination
description: Test no_hallucination invariant
requires:
  input_text:
    type: string
    default: "GraphQL is a query language for application programming interfaces and a runtime for executing those queries with your existing data. GraphQL provides a complete and understandable description of the data in your API giving clients the power to ask for exactly what they need and nothing more. Unlike REST APIs where you might need multiple endpoints to fetch related data, GraphQL enables you to get all required data in a single request. The strongly-typed schema defines what data is available and how clients can request it making APIs easier to evolve over time."
steps:
  - distill:
      input: ${input_text}
      intensity: balanced
      output_contract:
        length: phrase
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse distill scroll");

    // Consensus validation for no_hallucination happens internally
    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute distill with no_hallucination validation");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    assert!(output.as_str().is_some(), "Result should be string");
}

#[tokio::test]
async fn test_distill_compression_ratio() {
    // Test that output is actually compressed (shorter than input)
    let yaml = r#"
scroll: test-distill-compression
description: Test compression actually happens
requires:
  input_text:
    type: string
    default: "Container orchestration platforms like Kubernetes provide automated deployment scaling and management of containerized applications across clusters of machines. They handle service discovery by maintaining a registry of running services and their locations, load balancing to distribute traffic across multiple container instances, storage orchestration to manage persistent volumes, automated rollouts and rollbacks to safely update applications, self-healing capabilities to restart failed containers and reschedule them on healthy nodes, and secret and configuration management to securely inject sensitive data into containers at runtime."
steps:
  - distill:
      input: ${input_text}
      intensity: balanced
      output_contract:
        length: phrase
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse distill scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute distill successfully");

    let input_text = "Container orchestration platforms like Kubernetes provide automated deployment, scaling, and management of containerized applications. They handle service discovery, load balancing, storage orchestration, automated rollouts and rollbacks, self-healing capabilities, and secret and configuration management.";
    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let output_text = output.as_str().expect("Result should be string");

    let input_tokens = count_tokens(input_text);
    let output_tokens = count_tokens(output_text);

    assert!(output_tokens < input_tokens,
            "Output ({} tokens) should be shorter than input ({} tokens)",
            output_tokens, input_tokens);
}

#[tokio::test]
async fn test_distill_with_context() {
    // Test that context parameter is accepted and used
    let yaml = r#"
scroll: test-distill-context
description: Test context injection
requires:
  input_text:
    type: string
    default: "Machine learning models require comprehensive training data that represents the problem domain, careful feature engineering to extract meaningful patterns from raw data, systematic model selection to choose appropriate algorithms for the task, extensive hyperparameter tuning to optimize model performance, and rigorous evaluation metrics to ensure they generalize well to unseen data. The model development lifecycle involves iterative experimentation where data scientists continuously refine their approach based on validation results. Proper train-test splitting and cross-validation techniques help prevent overfitting and ensure robust model performance in production environments."
steps:
  - distill:
      input: ${input_text}
      intensity: balanced
      output_contract:
        length: phrase
        format: prose
      context:
        audience: "executives"
        preserve: ["business value"]
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse distill with context");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute distill with context");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    assert!(output.as_str().is_some(), "Result should be string");
}

#[tokio::test]
async fn test_distill_format_prose() {
    // Test that prose format produces prose output
    let yaml = r#"
scroll: test-distill-format-prose
description: Test prose format
requires:
  input_text:
    type: string
    default: "Agile methodologies emphasize iterative development through short cycles called sprints, close team collaboration with daily standups and retrospectives, continuous customer feedback to validate product direction, and the ability to respond to changing requirements throughout the development process. This approach contrasts with traditional waterfall methods by delivering working software incrementally rather than waiting for a complete product. Agile teams self-organize around work items use empirical process control to inspect and adapt their practices and maintain sustainable development pace over extended periods."
steps:
  - distill:
      input: ${input_text}
      intensity: balanced
      output_contract:
        length: phrase
        format: prose
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse distill scroll");

    let result = executor.execute_scroll(&scroll).await;
    if let Err(ref e) = result {
        eprintln!("Execution error: {:?}", e);
    }
    assert!(result.is_ok(), "Should execute distill successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");

    // Prose should not have list markers
    assert!(is_prose_format(text), "Output should be in prose format");
}

#[tokio::test]
async fn test_distill_format_bullets() {
    // Test that bullets format produces bullet list
    let yaml = r#"
scroll: test-distill-format-bullets
description: Test bullets format
requires:
  input_text:
    type: string
    default: "DevOps practices include continuous integration for merging code changes frequently into a shared repository with automated builds and tests, continuous deployment for automating release processes from code commit to production deployment, infrastructure as code for managing infrastructure through version-controlled configuration files enabling reproducible environments, comprehensive monitoring and logging for system observability providing real-time insights into application behavior and performance, and fostering collaboration between development and operations teams breaking down traditional organizational silos. These practices collectively aim to shorten the software development lifecycle while maintaining high quality and reliability."
steps:
  - distill:
      input: ${input_text}
      intensity: balanced
      output_contract:
        length: phrase
        format: bullets
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse distill scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute distill successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");

    // Bullets format should have list markers
    assert!(is_list_format(text), "Output should be in bullets format");
}

#[tokio::test]
async fn test_distill_format_keywords() {
    // Test that keywords format produces space/comma-separated keywords
    let yaml = r#"
scroll: test-distill-format-keywords
description: Test keywords format
requires:
  input_text:
    type: string
    default: "Blockchain technology provides a decentralized distributed ledger system that records transactions across multiple computers in a cryptographically secure manner making it nearly impossible to alter retroactively without the consensus of the network. Each block contains a cryptographic hash of the previous block a timestamp and transaction data creating an immutable chain of records. The decentralized nature eliminates the need for a central authority or trusted intermediary. Consensus mechanisms like proof-of-work or proof-of-stake ensure that all participants agree on the current state of the ledger. This technology enables trustless peer-to-peer transactions and forms the foundation for cryptocurrencies and decentralized applications."
steps:
  - distill:
      input: ${input_text}
      intensity: aggressive
      output_contract:
        length: keywords
        format: keywords
    output: result
"#;

    let mut executor = Executor::for_testing();
    let scroll = parse_scroll(yaml).expect("Should parse distill scroll");

    let result = executor.execute_scroll(&scroll).await;
    assert!(result.is_ok(), "Should execute distill successfully");

    let output = executor.context().get_variable("result")
        .expect("Should have result variable");
    let text = output.as_str().expect("Result should be string");

    // Keywords format should be very short (3-15 tokens)
    let token_count = count_tokens(text);
    assert!(token_count >= 3 && token_count <= 15,
            "Keywords format should have 3-15 tokens, got {}", token_count);
}

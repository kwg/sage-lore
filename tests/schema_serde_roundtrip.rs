// SPDX-License-Identifier: MIT
//! Test serde round-trip for new primitive step types.

use sage_lore::scroll::schema::{
    FsOperation, FsParams, FsStep, OnFail, PlatformOperation, PlatformParams, PlatformStep,
    RunParams, RunStep, TestOperation, TestParams, TestStep, VcsOperation, VcsParams, VcsStep,
};
use std::collections::HashMap;

#[test]
fn test_fs_step_serde_roundtrip() {
    let step = FsStep {
        fs: FsParams {
            operation: FsOperation::Read,
            path: "/tmp/test.txt".to_string(),
            content: None,
            dest: None,
        },
        output: Some("file_contents".to_string()),
        on_fail: OnFail::Halt,
    };

    // Serialize to YAML
    let yaml = serde_json::to_string(&step).expect("Failed to serialize FsStep");
    println!("FsStep YAML:\n{}", yaml);

    // Deserialize back
    let deserialized: FsStep = serde_yaml::from_str(&yaml).expect("Failed to deserialize FsStep");

    // Verify fields
    assert_eq!(deserialized.output, Some("file_contents".to_string()));
    match deserialized.fs.operation {
        FsOperation::Read => {}
        _ => panic!("Expected Read operation"),
    }
    assert_eq!(deserialized.fs.path, "/tmp/test.txt");
}

#[test]
fn test_vcs_step_serde_roundtrip() {
    let step = VcsStep {
        vcs: VcsParams {
            operation: VcsOperation::Commit,
            message: Some("Test commit".to_string()),
            files: Some(vec!["file1.txt".to_string(), "file2.txt".to_string()]),
            branch: None,
            name: None,
            set_upstream: None,
            scope: None,
            remote: None,
            target: None,
        },
        output: Some("commit_hash".to_string()),
        on_fail: OnFail::Halt,
    };

    let yaml = serde_json::to_string(&step).expect("Failed to serialize VcsStep");
    println!("VcsStep YAML:\n{}", yaml);

    let deserialized: VcsStep = serde_yaml::from_str(&yaml).expect("Failed to deserialize VcsStep");

    assert_eq!(deserialized.output, Some("commit_hash".to_string()));
    match deserialized.vcs.operation {
        VcsOperation::Commit => {}
        _ => panic!("Expected Commit operation"),
    }
    assert_eq!(deserialized.vcs.message, Some("Test commit".to_string()));
}

#[test]
fn test_test_step_serde_roundtrip() {
    let step = TestStep {
        test: TestParams {
            operation: TestOperation::Run,
            pattern: Some("**/*.test.ts".to_string()),
            files: None,
            config: None,
            tool: None,
            input: None,
        },
        output: Some("test_results".to_string()),
        on_fail: OnFail::Halt,
    };

    let yaml = serde_json::to_string(&step).expect("Failed to serialize TestStep");
    println!("TestStep YAML:\n{}", yaml);

    let deserialized: TestStep =
        serde_yaml::from_str(&yaml).expect("Failed to deserialize TestStep");

    assert_eq!(deserialized.output, Some("test_results".to_string()));
    match deserialized.test.operation {
        TestOperation::Run => {}
        _ => panic!("Expected Run operation"),
    }
}

#[test]
fn test_platform_step_serde_roundtrip() {
    let step = PlatformStep {
        platform: PlatformParams {
            operation: PlatformOperation::Env,
            var: Some("PATH".to_string()),
            command: None,
            number: None,
            payload: None,
            labels: None,
            body: None,
            head: None,
            base: None,
            title: None,
            description: None,
            strategy: None,
            state: None,
            milestone: None,
            assignee: None,
        },
        output: Some("env_vars".to_string()),
        on_fail: OnFail::Halt,
    };

    let yaml = serde_json::to_string(&step).expect("Failed to serialize PlatformStep");
    println!("PlatformStep YAML:\n{}", yaml);

    let deserialized: PlatformStep =
        serde_yaml::from_str(&yaml).expect("Failed to deserialize PlatformStep");

    assert_eq!(deserialized.output, Some("env_vars".to_string()));
    match deserialized.platform.operation {
        PlatformOperation::Env => {}
        _ => panic!("Expected Env operation"),
    }
}

#[test]
fn test_run_step_serde_roundtrip() {
    let mut args = HashMap::new();
    args.insert(
        "input".to_string(),
        serde_json::Value::String("test input".to_string()),
    );

    let step = RunStep {
        run: RunParams {
            scroll_path: "/scrolls/nested.scroll.yaml".to_string(),
            args: Some(args),
        },
        output: Some("nested_result".to_string()),
        on_fail: OnFail::Halt,
    };

    let yaml = serde_json::to_string(&step).expect("Failed to serialize RunStep");
    println!("RunStep YAML:\n{}", yaml);

    let deserialized: RunStep = serde_yaml::from_str(&yaml).expect("Failed to deserialize RunStep");

    assert_eq!(deserialized.output, Some("nested_result".to_string()));
    assert_eq!(
        deserialized.run.scroll_path,
        "/scrolls/nested.scroll.yaml"
    );
    assert!(deserialized.run.args.is_some());
}

#[test]
fn test_fs_step_write_operation() {
    let step = FsStep {
        fs: FsParams {
            operation: FsOperation::Write,
            path: "/tmp/output.txt".to_string(),
            content: Some("Hello, World!".to_string()),
            dest: None,
        },
        output: None,
        on_fail: OnFail::Continue,
    };

    let yaml = serde_json::to_string(&step).expect("Failed to serialize FsStep");
    let deserialized: FsStep = serde_yaml::from_str(&yaml).expect("Failed to deserialize FsStep");

    match deserialized.fs.operation {
        FsOperation::Write => {}
        _ => panic!("Expected Write operation"),
    }
    assert_eq!(deserialized.fs.content, Some("Hello, World!".to_string()));
}

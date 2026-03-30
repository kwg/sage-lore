use sage_lore::auth::{AuthError, AuthStore, ForgeType};
use tempfile::TempDir;

#[tokio::test]
async fn test_login_and_get_token() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let auth_path = temp_dir.path().join("auth.yaml");

    let store = AuthStore::with_path(auth_path);
    store.login(
        "test.com",
        ForgeType::GitHub,
        "https://test.com",
        "testuser",
        "testtoken",
    )?;

    let cred = store.get_credential("test.com")?;
    assert_eq!(cred.token, "testtoken");
    Ok(())
}

#[tokio::test]
async fn test_logout() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let auth_path = temp_dir.path().join("auth.yaml");

    let store = AuthStore::with_path(auth_path);
    store.login(
        "test.com",
        ForgeType::GitHub,
        "https://test.com",
        "testuser",
        "testtoken",
    )?;
    store.logout("test.com")?;

    let result = store.get_credential("test.com");
    assert!(matches!(result, Err(AuthError::NoCredentials(_))));
    Ok(())
}

#[tokio::test]
async fn test_get_credential() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let auth_path = temp_dir.path().join("auth.yaml");

    let store = AuthStore::with_path(auth_path);
    store.login(
        "test.com",
        ForgeType::GitHub,
        "https://test.com",
        "testuser",
        "testtoken",
    )?;

    let cred = store.get_credential("test.com")?;
    assert_eq!(cred.username, "testuser");
    assert_eq!(cred.url, "https://test.com");
    Ok(())
}

#[tokio::test]
async fn test_no_credentials() {
    let temp_dir = TempDir::new().unwrap();
    let auth_path = temp_dir.path().join("auth.yaml");

    let store = AuthStore::with_path(auth_path);
    let result = store.get_credential("nonexistent.com");
    assert!(matches!(result, Err(AuthError::NoCredentials(_))));
}

#[tokio::test]
async fn test_multiple_forges() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let auth_path = temp_dir.path().join("auth.yaml");

    let store = AuthStore::with_path(auth_path);
    store.login(
        "github.com",
        ForgeType::GitHub,
        "https://github.com",
        "ghuser",
        "ghtoken",
    )?;
    store.login(
        "gitlab.com",
        ForgeType::GitLab,
        "https://gitlab.com",
        "gluser",
        "gltoken",
    )?;

    let gh_cred = store.get_credential("github.com")?;
    assert_eq!(gh_cred.token, "ghtoken");
    assert_eq!(gh_cred.username, "ghuser");

    let gl_cred = store.get_credential("gitlab.com")?;
    assert_eq!(gl_cred.token, "gltoken");
    assert_eq!(gl_cred.username, "gluser");

    Ok(())
}

#[tokio::test]
async fn test_login_overwrites_existing() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let auth_path = temp_dir.path().join("auth.yaml");

    let store = AuthStore::with_path(auth_path);
    store.login(
        "test.com",
        ForgeType::GitHub,
        "https://test.com",
        "olduser",
        "oldtoken",
    )?;
    store.login(
        "test.com",
        ForgeType::GitHub,
        "https://test.com",
        "newuser",
        "newtoken",
    )?;

    let cred = store.get_credential("test.com")?;
    assert_eq!(cred.username, "newuser");
    assert_eq!(cred.token, "newtoken");
    Ok(())
}

#[tokio::test]
async fn test_logout_nonexistent_host() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let auth_path = temp_dir.path().join("auth.yaml");

    let store = AuthStore::with_path(auth_path);
    // Logging out a nonexistent host should not error
    let result = store.logout("nonexistent.com");
    assert!(result.is_ok());
    Ok(())
}

#[tokio::test]
async fn test_forge_types() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let auth_path = temp_dir.path().join("auth.yaml");

    let store = AuthStore::with_path(auth_path);

    store.login(
        "forgejo.example.com",
        ForgeType::Forgejo,
        "https://forgejo.example.com",
        "user",
        "token",
    )?;
    store.login(
        "gitea.example.com",
        ForgeType::Gitea,
        "https://gitea.example.com",
        "user",
        "token",
    )?;

    let forgejo_cred = store.get_credential("forgejo.example.com")?;
    assert!(matches!(forgejo_cred.forge_type, ForgeType::Forgejo));

    let gitea_cred = store.get_credential("gitea.example.com")?;
    assert!(matches!(gitea_cred.forge_type, ForgeType::Gitea));

    Ok(())
}

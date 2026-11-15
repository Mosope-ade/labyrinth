use super::{test_support::*, *};
use serde_json::json;
use uuid::Uuid;
#[tokio::test]
async fn signed_user_claims_are_required_and_untrusted_roles_never_grant_identity() {
    let source = TestIssuer::start().await;
    let verifier = source.verifier().await.unwrap();
    let actor = Uuid::new_v4();
    let valid = claims(actor);
    assert_eq!(
        verifier
            .verify(&source.signer.token(&valid))
            .await
            .unwrap()
            .id(),
        actor
    );
    for (key, value) in [
        ("iss", json!("https://other.supabase.co/auth/v1")),
        ("aud", json!("anon")),
        ("role", json!("service_role")),
        ("exp", json!(1)),
        ("nbf", json!(99999999999u64)),
        ("iat", json!(99999999999u64)),
        ("sub", json!("not-a-uuid")),
        ("is_anonymous", json!(true)),
        ("session_id", json!(Uuid::nil())),
    ] {
        let mut changed = valid.clone();
        changed[key] = value;
        assert_eq!(
            verifier
                .verify(&source.signer.token(&changed))
                .await
                .unwrap_err(),
            AuthError::InvalidToken,
            "claim {key}"
        );
    }
    for key in [
        "sub",
        "exp",
        "iat",
        "iss",
        "aud",
        "session_id",
        "is_anonymous",
    ] {
        let mut changed = valid.clone();
        changed.as_object_mut().unwrap().remove(key);
        assert_eq!(
            verifier
                .verify(&source.signer.token(&changed))
                .await
                .unwrap_err(),
            AuthError::InvalidToken,
            "missing {key}"
        );
    }
}
#[tokio::test]
async fn bad_signatures_algorithms_unknown_kids_and_malformed_tokens_reject_without_fetch_storm() {
    let source = TestIssuer::start().await;
    let verifier = source.verifier().await.unwrap();
    let valid = claims(Uuid::new_v4());
    let forged = Signer::new(&source.signer.kid);
    assert_eq!(
        verifier.verify(&forged.token(&valid)).await.unwrap_err(),
        AuthError::InvalidToken
    );
    let mut h = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
    h.kid = Some(source.signer.kid.clone());
    let hmac = jsonwebtoken::encode(
        &h,
        &valid,
        &jsonwebtoken::EncodingKey::from_secret(b"not-a-project-key"),
    )
    .unwrap();
    assert_eq!(
        verifier.verify(&hmac).await.unwrap_err(),
        AuthError::InvalidToken
    );
    let unknown = Signer::new("unknown");
    for token in [
        "invalid".to_string(),
        "x".repeat(16385),
        unknown.token(&valid),
    ] {
        assert_eq!(
            verifier.verify(&token).await.unwrap_err(),
            AuthError::InvalidToken
        );
    }
    assert_eq!(source.hits(), 1);
}
#[tokio::test]
async fn rotation_replaces_expired_keys_and_provider_outage_fails_closed_after_ttl() {
    let source = TestIssuer::start().await;
    let verifier = source.verifier().await.unwrap();
    let valid = claims(Uuid::new_v4());
    let old = source.signer.token(&valid);
    let next = Signer::new("key-two");
    source.replace_keys(vec![next.jwk.clone()]);
    verifier.expire_for_test().await;
    verifier.verify(&next.token(&valid)).await.unwrap();
    assert_eq!(
        verifier.verify(&old).await.unwrap_err(),
        AuthError::InvalidToken
    );
    source.fail();
    verifier.verify(&next.token(&valid)).await.unwrap();
    verifier.expire_for_test().await;
    assert_eq!(
        verifier.verify(&next.token(&valid)).await.unwrap_err(),
        AuthError::Unavailable
    );
    let hits = source.hits();
    assert_eq!(
        verifier.verify(&next.token(&valid)).await.unwrap_err(),
        AuthError::Unavailable
    );
    assert_eq!(source.hits(), hits);
}
#[tokio::test]
async fn empty_duplicate_or_inappropriate_jwks_cannot_bootstrap_authentication() {
    let source = TestIssuer::start().await;
    source.replace_keys(vec![]);
    assert!(matches!(
        source.verifier().await,
        Err(AuthError::Unavailable)
    ));
    source.replace_keys(vec![source.signer.jwk.clone(), source.signer.jwk.clone()]);
    assert!(matches!(
        source.verifier().await,
        Err(AuthError::Unavailable)
    ));
    let mut mismatch = source.signer.jwk.clone();
    mismatch["alg"] = json!("RS256");
    source.replace_keys(vec![mismatch]);
    assert!(matches!(
        source.verifier().await,
        Err(AuthError::Unavailable)
    ));
    let mut jwk = source.signer.jwk.clone();
    jwk["use"] = json!("enc");
    source.replace_keys(vec![jwk]);
    assert!(matches!(
        source.verifier().await,
        Err(AuthError::Unavailable)
    ));
}
#[test]
fn issuer_configuration_never_uses_plaintext_credentials_queries_or_token_selected_urls() {
    for issuer in [
        "http://localhost/auth/v1",
        "https://user:password@host/auth/v1",
        "https://host/auth/v1?token=secret",
        "https://host/auth/v1/",
        "https://host/auth/v1#fragment",
    ] {
        assert!(AuthConfig::new(issuer).is_err());
    }
    assert!(AuthConfig::new(ISSUER).is_ok());
}

#[tokio::test]
async fn rs256_public_keys_verify_real_rsa_signatures() {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use rsa::{RsaPrivateKey, pkcs1::EncodeRsaPrivateKey, traits::PublicKeyParts};
    let source = TestIssuer::start().await;
    let private = RsaPrivateKey::new(&mut rand_core::OsRng, 2048).unwrap();
    let public = private.to_public_key();
    source.replace_keys(vec![json!({"kty":"RSA","alg":"RS256","kid":"rsa-key","use":"sig","key_ops":["verify"],"n":URL_SAFE_NO_PAD.encode(public.n().to_bytes_be()),"e":URL_SAFE_NO_PAD.encode(public.e().to_bytes_be())})]);
    let verifier = source.verifier().await.unwrap();
    let actor = Uuid::new_v4();
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some("rsa-key".into());
    let token = jsonwebtoken::encode(
        &header,
        &claims(actor),
        &jsonwebtoken::EncodingKey::from_rsa_der(private.to_pkcs1_der().unwrap().as_bytes()),
    )
    .unwrap();
    assert_eq!(verifier.verify(&token).await.unwrap().id(), actor);
}

#[tokio::test]
async fn concurrent_key_misses_coalesce_to_one_bounded_refresh() {
    let source = TestIssuer::start().await;
    let verifier = std::sync::Arc::new(source.verifier().await.unwrap());
    verifier.expire_for_test().await;
    let signer = Signer::new("unknown-key");
    let token = signer.token(&claims(Uuid::new_v4()));
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let verifier = verifier.clone();
        let token = token.clone();
        tasks.push(tokio::spawn(async move { verifier.verify(&token).await }));
    }
    for task in tasks {
        assert_eq!(task.await.unwrap().unwrap_err(), AuthError::InvalidToken);
    }
    assert_eq!(source.hits(), 2);
}

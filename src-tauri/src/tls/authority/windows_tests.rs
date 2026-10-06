//! Exercise the production root parameters in an exclusive in-memory Windows
//! root store. Never add certificates to CurrentUser or LocalMachine stores.
use super::*;
use windows_sys::Win32::Security::Cryptography::*;

#[test]
fn windows_root_constraints_reject_real_domains_and_ips_through_an_intermediate() {
    let root_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
    let params = parameters().unwrap();
    let root = params.self_signed(&root_key).unwrap();
    let issuer = rcgen::Issuer::new(params, &root_key);
    let intermediate_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
    let mut intermediate_params = CertificateParams::default();
    intermediate_params
        .distinguished_name
        .push(DnType::CommonName, "DEVONE test intermediate");
    intermediate_params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    intermediate_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let intermediate = intermediate_params
        .signed_by(&intermediate_key, &issuer)
        .unwrap();
    let intermediate_issuer = rcgen::Issuer::new(intermediate_params, &intermediate_key);
    unsafe {
        let roots = CertOpenStore(
            CERT_STORE_PROV_MEMORY,
            0,
            0,
            CERT_STORE_CREATE_NEW_FLAG,
            std::ptr::null(),
        );
        let additional = CertOpenStore(
            CERT_STORE_PROV_MEMORY,
            0,
            0,
            CERT_STORE_CREATE_NEW_FLAG,
            std::ptr::null(),
        );
        assert!(!roots.is_null() && !additional.is_null());
        for (store, cert) in [(roots, root.der()), (additional, intermediate.der())] {
            assert_ne!(
                CertAddEncodedCertificateToStore(
                    store,
                    X509_ASN_ENCODING,
                    cert.as_ptr(),
                    cert.len() as u32,
                    CERT_STORE_ADD_NEW,
                    std::ptr::null_mut()
                ),
                0
            );
        }
        let mut config: CERT_CHAIN_ENGINE_CONFIG = std::mem::zeroed();
        config.cbSize = std::mem::size_of_val(&config) as u32;
        config.hExclusiveRoot = roots;
        config.dwFlags = CERT_CHAIN_DISABLE_AIA | CERT_CHAIN_CACHE_ONLY_URL_RETRIEVAL;
        let mut engine = std::ptr::null_mut();
        assert_ne!(CertCreateCertificateChainEngine(&config, &mut engine), 0);
        let leaf_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
        for (hostname, permitted) in [
            ("site.test", true),
            ("example.com", false),
            ("127.0.0.1", false),
            ("::1", false),
        ] {
            let mut leaf_params = CertificateParams::new(vec![hostname.into()]).unwrap();
            leaf_params
                .extended_key_usages
                .push(rcgen::ExtendedKeyUsagePurpose::ServerAuth);
            let leaf = leaf_params
                .signed_by(&leaf_key, &intermediate_issuer)
                .unwrap();
            let context = CertCreateCertificateContext(
                X509_ASN_ENCODING,
                leaf.der().as_ptr(),
                leaf.der().len() as u32,
            );
            assert!(!context.is_null());
            let mut params: CERT_CHAIN_PARA = std::mem::zeroed();
            params.cbSize = std::mem::size_of_val(&params) as u32;
            let mut chain = std::ptr::null_mut();
            assert_ne!(
                CertGetCertificateChain(
                    engine,
                    context,
                    std::ptr::null(),
                    additional,
                    &params,
                    CERT_CHAIN_DISABLE_AIA
                        | CERT_CHAIN_CACHE_ONLY_URL_RETRIEVAL
                        | CERT_CHAIN_DISABLE_AUTH_ROOT_AUTO_UPDATE,
                    std::ptr::null(),
                    &mut chain
                ),
                0
            );
            let status = (*chain).TrustStatus.dwErrorStatus;
            CertFreeCertificateChain(chain);
            CertFreeCertificateContext(context);
            if permitted {
                assert_eq!(status, 0, "{hostname}");
            } else {
                assert_ne!(
                    status
                        & (CERT_TRUST_HAS_NOT_PERMITTED_NAME_CONSTRAINT
                            | CERT_TRUST_HAS_EXCLUDED_NAME_CONSTRAINT),
                    0,
                    "{hostname}: {status:#x}"
                );
            }
        }
        CertFreeCertificateChainEngine(engine);
        CertCloseStore(additional, 0);
        CertCloseStore(roots, 0);
    }
}

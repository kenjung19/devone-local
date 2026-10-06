# Public certificate fixtures

These are public Caddy development CA certificates from two disposable
acceptance Homes. They have the same subject and different keys/encoded
identities. No private key is included. They are not installed into Windows
Root by automatic tests.

The Windows unit test loads them into a disposable CryptoAPI **memory** store
to test the same exact-DER predicate used by production Root lookup. It also
changes one signature byte in a copied certificate to retain issuer/serial
while changing DER; that intentionally invalid certificate is used only to
test identity matching, never accepted for TLS. Real TLS validation uses a
fresh Home CA and rejects the unrelated fixture.

# Authentication test certificates

These keys and certificates are public test data. Never use them outside local tests.

`ca.pem` signs the server and client certificates. `server.pem` contains a server certificate and key for 127.0.0.1 and localhost. `client.pem` contains a client-auth certificate and key. Certificates expire in October 2036. The CA private key is not retained.

The test harness adds this CA to test clients only. Production builds do not include the fixture trust hook or the in-memory credential store. Tests do not modify the OS trust store or Keychain.

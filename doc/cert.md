## Overview
For a typical self signed cert to work, three files are needed, but one more file will be generated during the process.

 - `key.pem` is the private RSA key of the server.
 - `cert.pem` the public cert of the server.
 - `ca_cert.pem` is the public cert of the CA who issues the `cert.pem`.
 - `ca_key.pem` is the private RSA key of the CA. Although it is not used for client/server to run, it is used to generate `ca_cert.pem` and issue `cert.pem`.

## Steps

There are many ways to get these files. Here is one of them.

To generate the server private key `encrypted_key.pem` along with its Certificate Signing Request `cert_csr.pem`.
For non-self signed cert, the `cert_csr.pem` will be sent to CA, and CA will return the cert back as the response.
But in our case, we actor like a CA as well. So we can self sign the cert.

```
openssl req -newkey rsa:2048 -keyout encrypted_key.pem -out cert_csr.pem
```

To generate the CA private key `encrypted_ca_key.pem` along with its cert `ca_cert.pem`.

```
openssl req -x509 -newkey rsa:2048 -keyout encrypted_ca_key.pem -out ca_cert.pem
```

Create a file `extentions.conf`, and put those into it.
`subjectAltName` lists all domains belongs to the cert.
It will be part of the cert, so client can verify the cert matches the domain it requests.

```
subjectAltName=@my_subject_alt_names

[ my_subject_alt_names ]
DNS.1 = *.example.com
```

Now take the CSR, CA cert and CA key, we can get the cert for our server.

```
openssl x509 -req -in cert_csr.pem -CA ca_cert.pem -CAkey encrypted_ca_key.pem -CAcreateserial -extfile extentions.conf -out cert.pem
```

We can verify the cert with this command.

```
openssl verify -CAfile ca_cert.pem cert.pem
```

Finally, we need to decrypt the server private key because our program doesn't support encrypted key.

```
openssl rsa -in encrypted_key.pem -out key.pem
```

That is all. You probably want to keep the `encrypted_ca_key.pem` to sign more cert.

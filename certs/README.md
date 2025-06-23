# SSL Certificates for Bitquery Kafka Streaming

This directory contains the SSL certificates required for secure communication with Bitquery's Kafka brokers.

## Files Overview

- **`server.cer.pem`** - Server certificate (CA certificate)
- **`client.cer.pem`** - Client certificate for authentication
- **`client.key.pem`** - Client private key (keep secure!)

## Current Status

✅ **SSL Connection Confirmed**: Testing shows that Bitquery's Kafka brokers **require SSL connections**. Non-SSL attempts result in:
```
Disconnected while requesting ApiVersion: might be caused by incorrect security.protocol configuration (connecting to a SSL listener?)
```

⚠️ **Client Certificates Needed**: The current client certificate files (`client.cer.pem`, `client.key.pem`) are **placeholder certificates** and will **NOT work** with Bitquery's production Kafka brokers. The server certificate (`server.cer.pem`) has been updated with the real CA certificate from Bitquery.

## Obtaining Production Certificates

To get the proper SSL certificates for Bitquery streaming:

1. **Contact Bitquery Support**
   - Email: support@bitquery.io
   - Request SSL certificates for Kafka streaming access
   - Provide your account details and subscription information

2. **Certificate Requirements**
   - You need client certificates that are signed by Bitquery's Certificate Authority
   - The certificates must match your Bitquery account credentials
   - Certificates are typically provided in PEM format

3. **Replace the Placeholder Certificates**
   - Replace the files in this directory with the certificates provided by Bitquery
   - Ensure file permissions are secure (readable only by the service user)
   - Restart the zola-streams service after updating certificates

## Security Best Practices

- **Never commit real certificates to version control**
- Use environment variables or secure secret management for certificate paths
- Restrict file permissions: `chmod 600 client.key.pem`
- Regularly rotate certificates as per Bitquery's recommendations

## Configuration

The SSL certificate paths are configured in `src/config.rs`:

```rust
pub struct SslConfig {
    pub ca_cert: String,      // Points to server.cer.pem
    pub client_key: String,   // Points to client.key.pem  
    pub client_cert: String,  // Points to client.cer.pem
}
```

You can override these paths via environment variables or configuration files.

## Troubleshooting

If you see SSL-related errors like:
```
ssl.ca.location failed: error:05880009:x509 certificate routines::PEM lib
```

This typically means:
1. The certificate files are not valid PEM format
2. The certificates are test/placeholder certificates (like the current ones)
3. The certificate files are missing or have incorrect permissions
4. The certificates are not signed by the correct CA for Bitquery

## Testing Connection

Once you have valid certificates, test the connection with:

```bash
# Run the service and check logs
./target/release/zola-streams

# Check observability endpoints
curl http://localhost:8080/health
curl http://localhost:8080/ready
```

The service should successfully connect to Bitquery Kafka brokers and start processing events.

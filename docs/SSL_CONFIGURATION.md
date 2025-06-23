# SSL Configuration Guide for Bitquery Kafka Streaming

This guide explains how to configure SSL connections for Bitquery's Kafka streaming service, based on the official [Bitquery documentation](https://docs.bitquery.io/docs/streams/kafka-streaming-concepts/).

## Connection Methods

Bitquery supports two connection methods:

### 1. SASL_PLAINTEXT (Recommended)

**Port:** `9092`  
**SSL Required:** No  
**Preferred Method:** ✅ Yes

This is Bitquery's **preferred connection method** as it's simpler and doesn't require certificate management.

**Configuration:**
```rust
KafkaConfig {
    brokers: vec![
        "rpk0.bitquery.io:9092".to_string(),
        "rpk1.bitquery.io:9092".to_string(),
        "rpk2.bitquery.io:9092".to_string(),
    ],
    enable_ssl: false,
    username: "your_username".to_string(),
    password: "your_password".to_string(),
    // ... other config
}
```

**Generated rdkafka Configuration:**
```
bootstrap.servers: rpk0.bitquery.io:9092,rpk1.bitquery.io:9092,rpk2.bitquery.io:9092
security.protocol: SASL_PLAINTEXT
sasl.mechanism: SCRAM-SHA-512
sasl.username: your_username
sasl.password: your_password
```

### 2. SASL_SSL (Alternative)

**Port:** `9093`  
**SSL Required:** Yes  
**Certificates Required:** `client.key.pem`, `client.cer.pem`, `server.cer.pem`

**Configuration:**
```rust
KafkaConfig {
    brokers: vec![
        "rpk0.bitquery.io:9093".to_string(),
        "rpk1.bitquery.io:9093".to_string(),
        "rpk2.bitquery.io:9093".to_string(),
    ],
    enable_ssl: true,
    ssl: SslConfig {
        ca_cert: "certs/server.cer.pem".to_string(),
        client_key: "certs/client.key.pem".to_string(),
        client_cert: "certs/client.cer.pem".to_string(),
    },
    username: "your_username".to_string(),
    password: "your_password".to_string(),
    // ... other config
}
```

**Generated rdkafka Configuration:**
```
bootstrap.servers: rpk0.bitquery.io:9093,rpk1.bitquery.io:9093,rpk2.bitquery.io:9093
security.protocol: SASL_SSL
sasl.mechanism: SCRAM-SHA-512
sasl.username: your_username
sasl.password: your_password
ssl.ca.location: certs/server.cer.pem
ssl.key.location: certs/client.key.pem
ssl.certificate.location: certs/client.cer.pem
ssl.endpoint.identification.algorithm: none
```

## Certificate Management

### Obtaining Certificates

Certificates for SASL_SSL connections are provided by the Bitquery support team:

1. **Contact Bitquery Support** at [support@bitquery.io](mailto:support@bitquery.io)
2. **Request SSL certificates** for your account
3. **Receive three files:**
   - `client.key.pem` - Your private key
   - `client.cer.pem` - Your client certificate
   - `server.cer.pem` - CA certificate (server certificate)

### Current Certificate Status

```
certs/
├── server.cer.pem    ✅ Real Bitquery CA certificate (obtained)
├── client.cer.pem    ❌ Placeholder (need real certificate from Bitquery)
└── client.key.pem    ❌ Placeholder (need real certificate from Bitquery)
```

### Certificate Validation

You can validate certificates using OpenSSL:

```bash
# Validate server certificate
openssl x509 -in certs/server.cer.pem -text -noout

# Verify client certificate matches private key
openssl x509 -noout -modulus -in certs/client.cer.pem | openssl md5
openssl rsa -noout -modulus -in certs/client.key.pem | openssl md5
# Both commands should output the same hash
```

## Environment Configuration

Set the following environment variables to override default configuration:

```bash
# Authentication (required)
export BITQUERY_USERNAME="your_username"
export BITQUERY_PASSWORD="your_password"

# Broker configuration (optional, defaults to SASL_PLAINTEXT)
export KAFKA_BOOTSTRAP_SERVERS="rpk0.bitquery.io:9092,rpk1.bitquery.io:9092,rpk2.bitquery.io:9092"

# For SSL connections, update the config programmatically or use config files
```

## Authentication Details

- **SASL Mechanism:** `SCRAM-SHA-512`
- **Group ID Format:** Must start with your username (automatically handled by `Config::new()`)
- **Credentials:** Provided by Bitquery support team

## Testing Connection

### Quick Test with SASL_PLAINTEXT (Recommended)

```bash
# Build the project
cargo build --release

# Set credentials
export BITQUERY_USERNAME="your_username"
export BITQUERY_PASSWORD="your_password"

# Run the service
cargo run
```

### Test with SSL (if certificates available)

```rust
// Update main.rs or config
config.kafka.enable_ssl = true;
config.kafka.brokers = vec![
    "rpk0.bitquery.io:9093".to_string(),
    "rpk1.bitquery.io:9093".to_string(),
    "rpk2.bitquery.io:9093".to_string(),
];
```

## Troubleshooting

### Common Issues

1. **Connection Refused on Port 9093**
   - Ensure you have valid SSL certificates
   - Try SASL_PLAINTEXT on port 9092 instead

2. **Authentication Failed**
   - Verify username and password with Bitquery support
   - Ensure group_id starts with your username

3. **SSL Certificate Errors**
   - Verify certificates are valid and not placeholders
   - Contact Bitquery support for real certificates

4. **Hostname Verification Errors**
   - The service disables hostname verification (`ssl.endpoint.identification.algorithm: none`)
   - This is acceptable for Bitquery's setup

### Debug Tips

1. **Enable Debug Logging:**
   ```bash
   RUST_LOG=debug cargo run
   ```

2. **Check Configuration:**
   ```bash
   # The service logs the connection method on startup
   # Look for: "Configuring SSL/TLS encryption" or "Using SASL-only authentication"
   ```

3. **Test Network Connectivity:**
   ```bash
   # Test SASL_PLAINTEXT
   telnet rpk0.bitquery.io 9092
   
   # Test SASL_SSL
   openssl s_client -connect rpk0.bitquery.io:9093
   ```

## Next Steps

1. **For Production:** Use SASL_PLAINTEXT (port 9092) for simplicity
2. **For Enhanced Security:** Obtain real certificates from Bitquery support for SASL_SSL (port 9093)
3. **Contact Support:** Reach out to [support@bitquery.io](mailto:support@bitquery.io) for:
   - Real client certificates for SSL
   - Username/password credentials
   - Topic access permissions

## References

- [Bitquery Kafka Streaming Documentation](https://docs.bitquery.io/docs/streams/kafka-streaming-concepts/)
- [Bitquery SSL Configuration Examples](https://docs.bitquery.io/docs/streams/kafka-streaming-concepts/#example-ssl-connection-configuration-sasl--ssl)
- [rdkafka Configuration Documentation](https://github.com/edenhill/librdkafka/blob/master/CONFIGURATION.md)

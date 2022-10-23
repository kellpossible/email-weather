+++
title = "Deployment"
description = "Deployment instructions (for those who want to run their own instance of the service)"
weight = 2
+++

## Secrets

### `CLIENT_SECRET` | `secrets/clientsecret.json`

### `TOKEN_CACHE` | `secrets/tokencache.json`

### `ADMIN_PASSWORD_HASH` | `secrets/admin_password_hash`

The administrator password to be used for viewing debug/log information about the application. If this secret is not provided, then the debug/log http interface is disabled. **Note**: this is designed to be used when the service is running behind a proxy providing TLS, otherwise the user password will be transmitted in plain text.

The password needs to be hashed using [bcrypt](https://en.wikipedia.org/wiki/Bcrypt), you can use the provided `admin-password-hash` utility to create your own:

```bash
$ cargo run -p admin-password-hash
Enter password to be hashed:🔒
$ $2b$10$sl6AVe96a.smPQW1EHlEtuEyD4rxWvjLIIvDmKgghteQXqjaGDdka
```

## Environment Variables

### `OVERWRITE_TOKEN_CACHE`

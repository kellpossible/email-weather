+++
title = "Deployment"
description = "Deployment Instructions"
weight = 2
+++

This document provides instructions and information to enable someone to deploy their own personal instance of this service.

## OAUTH2, IMAP and SMTP for Email

The `email-weather` service relies on having access to an email account to receive and reply to emails. Currently only the Gmail service is being tested and supported, but if you'd like to deploy it with another service, feel free to [post an issue](https://github.com/kellpossible/email-weather/issues) to request support for your email provider of choice and we can investigate supporting it.

### Gmail

In order to access a Gmail account with this service, you will first need to enable the following features in your Gmail account:

+ [Turn on IMAP](https://support.google.com/mail/answer/7126229#zippy=%2Cstep-check-that-imap-is-turned-on).
+ Turn on SMTP (TODO).

You can read more about how this is used in the [IMAP, POP, and SMTP Gmail Documentation](https://developers.google.com/gmail/imap/imap-smtp).

You will also need to set up OAUTH2:

1. Register your service as an application.
2. Authenticate with your Gmail account. It is recommended to do this locally, and then provide the generated token cache to your server via the `TOKEN_CACHE` secret (see [Secrets](#secrets)).

Further Reading:

+ https://developers.google.com/gmail/imap/xoauth2-protocol
+ https://developers.google.com/identity/protocols/oauth2


## Secrets

### `CLIENT_SECRET` | `secrets/client_secret.json`

### `TOKEN_CACHE` | `secrets/token_cache.json`

### `ADMIN_PASSWORD_HASH` | `secrets/admin_password_hash`

The administrator password to be used for viewing debug/log information about the application. If this secret is not provided, then the debug/log http interface is disabled. **Note**: this is designed to be used when the service is running behind a proxy providing TLS, otherwise the user password will be transmitted in plain text.

The password needs to be hashed using [bcrypt](https://en.wikipedia.org/wiki/Bcrypt), you can use the provided `admin-password-hash` utility to create your own:

```bash
$ cargo run -p admin-password-hash
Enter password to be hashed:🔒
$2b$10$sl6AVe96a.smPQW1EHlEtuEyD4rxWvjLIIvDmKgghteQXqjaGDdka
```

Beware, the `$` signs may mess with your shell, and require escaping, or the use of single quote, for example: `ADMIN_PASSWORD_HASH='$2b$10$sl6AVe96a.smPQW1EHlEtuEyD4rxWvjLIIvDmKgghteQXqjaGDdka'`.

## Environment Variables

### `OVERWRITE_TOKEN_CACHE`

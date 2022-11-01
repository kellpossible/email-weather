+++
title = "Deployment"
description = "Deployment Instructions"
weight = 2
+++

This document provides instructions and information to enable someone to deploy their own personal instance of this service.

## Logs

This service is currently designed around a deployment using [fly.io](https://fly.io/). 

Fly [has limited logging capabilities](https://community.fly.io/t/getting-fly-logs/4011) (the history is severly limited), setting up [fly-log-shipper](https://github.com/superfly/fly-log-shipper) is unecessarily complicated, so it was decided to build a system for viewing application logs using the http server running on this server that was already required for OAUTH2 redirects. 

These logs are available on the route `/logs/`, and are stored in the `data` directory as specified in [Options](#options). Accessing this route requires basic authentication using the user `admin`, and the password who's hash is specified in [Secrets](#secrets). **Be aware** that this transmits the password in plain text and is not appropriate for a plain http connection.

## OAUTH2, IMAP and SMTP for Email

The `email-weather` service relies on having access to an email account to receive and reply to emails. Currently only the Gmail service is being tested and supported, but if you'd like to deploy it with another service, feel free to [post an issue](https://github.com/kellpossible/email-weather/issues) to request support for your email provider of choice and we can investigate supporting it. The code for many of the alternative methods of OAUTH2 authentication has already been implemented (currently unused) during the quest to figure out reliable access to Gmail.

### Gmail

In order to access a Gmail account with this service, you will first need to enable the following features in your Gmail account:

+ [Turn on IMAP](https://support.google.com/mail/answer/7126229#zippy=%2Cstep-check-that-imap-is-turned-on).
+ Turn on SMTP (TODO).

You can read more about how this is used in the [IMAP, POP, and SMTP Gmail Documentation](https://developers.google.com/gmail/imap/imap-smtp).

You will also need to set up OAUTH2:

1. Register your service as an application.
2. Publish your service (ignore the warnings about verification)
3. Authenticate with your Gmail account using the link provided in the logs.

Further Reading:

+ <https://developers.google.com/gmail/imap/xoauth2-protocol>
+ <https://developers.google.com/identity/protocols/oauth2>


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

## Options

Options for running the application are specified in [ron](https://github.com/ron-rs/ron) format. See `struct Options` in [options.rs](https://github.com/kellpossible/email-weather/blob/main/src/options.rs) for description of the available options.

By default, the application will attempt to load options from the file `options.rs`, however you may also elect to one of the following:

+ Specify a custom path to options RON file in environment variable `OPTIONS`. (e.g. `OPTIONS="path/to/options.ron"`).
+ Specify options in RON format with the value for the environment variable `OPTIONS`. (e.g. `OPTIONS="Options(...)"`).

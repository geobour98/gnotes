# Security policy

gnotes stores private notes, so security reports are very welcome.

## Reporting a vulnerability

Please **don't post the details publicly** (e.g. in an issue). Contact the maintainer privately
first; if you have no other way to reach them, open an issue saying only that you have a security
report and would like a private contact. Include what you found, how to reproduce it, and what an
attacker could do with it.

You'll get a reply as soon as possible. Fixes are released before the details are made public.

## Scope

In scope, for example:

- reading or changing vault contents without the password
- decrypted content written to disk (other than exports or files the user saves)
- a note (e.g. an imported one) that can run script, read files, or make the app navigate or
  connect somewhere
- weaknesses in key derivation or how the database is encrypted

Out of scope: attacks that need the password, or malware already running as the user while the
vault is unlocked. See "What it does not protect against" in the [README](README.md#security).

Only the latest version is supported.

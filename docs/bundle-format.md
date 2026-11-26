# `.blackout` bundle format v1

The file is a Zstandard stream containing a POSIX tar archive. The archive contains regular project files and one `BLACKOUT-MANIFEST.json` entry. No symlink, hardlink, device, FIFO, or sparse-file entries are accepted by the reader.

The manifest has `format_version`, `project`, `ecosystem`, `target`, `files`, and `validation` fields. Every file record has a UTF-8 relative path, lowercase SHA-256, uncompressed byte size, and permission bits. Restore strips special bits, preserves executable bits, and ensures the owner can read/write restored files. Validation fields are nullable and remain unset unless a future command records an observed result.

Limits in v1: 100,000 regular files; 1 GiB per file; 8 GiB aggregate uncompressed file bytes; 1 MiB manifest. The reader rejects absolute paths, `.`/`..` components, duplicate paths, unexpected files, missing files, size mismatches, digest mismatches, and existing restore destinations. Extraction occurs only after full archive and checksum validation.

This format is not OCI. Checksums do not authenticate the publisher. Future incompatible formats require a version increment.

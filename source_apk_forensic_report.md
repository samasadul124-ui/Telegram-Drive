# TG Cloud APK forensic report — v1.2.0

## Scope
Analysis of the supplied working APK `com.telegram.cloud_1.2.0 (1).apk`.
The APK itself was not modified.

## High-confidence identification
- Package namespace: `com.telegram.cloud`
- Version string present in DEX: `1.2.0`
- DEX size: 7,680,756 bytes
- App-owned classes: 1,697 class definitions
- App-owned fields: 5,827
- App-owned methods: 7,993
- Kotlin metadata identifies Gradle build system and Kotlin Android plugin.
- Kotlin tooling metadata: Kotlin plugin 1.9.24; Gradle 8.7; Android source/target compatibility 17.
- APK metadata reports Android Gradle Plugin 8.5.0.

## Native components
`libtelegramcloud_core.so` exists for ARM64 and ARMv7.
It exports 12 JNI entry points under `Java_com_telegram_cloud_NativeLib_*`.
The ARM64 library is AArch64 ELF, dynamically linked against Android system libraries.
The ELF `.comment` reports Android clang 14.0.7 / LLD 14.0.7.
No DWARF debug sections were found, so original native source-level debug information is not present.

### JNI API recovered from exports
- nativeInit()
- nativeOpenDatabase(path, ...)
- nativeCloseDatabase()
- nativeExportBackup(path)
- nativeImportBackup(path)
- nativeImportEncryptedBackup(path, password)
- nativeStartDownload(url, destination)
- nativeStartTransfer(payload)
- nativeCancelTransfer(id)
- nativeStopDownload(id)
- nativeStartUpload(file, target)
- nativeGetDownloadStatus(id)

The exact Java signatures are listed in `dex/method_field_inventory.txt`.

## Native implementation evidence
The native library contains:
- libcurl API symbols and MIME/form APIs
- OpenSSL cryptographic/TLS symbols
- SQLCipher/SQLite symbols
- SHA-256 and MD5 functions
- filesystem operations (copy, rename, remove, create directories)
- JSON-driven transfer parsing (`parseTransferRequest`)
- native backup import/export handling
- progress/completion/failure callbacks

Native strings explicitly reference Telegram Cloud's Java callback/dispatcher and backup/configuration handling.

## Android architecture recovered from class names
Major first-party modules include:

### Data/local
- SQLCipherDatabase
- CloudDatabase / CloudFileDao / CloudFileEntity
- UploadTaskDao / UploadTaskEntity
- DownloadTaskDao / DownloadTaskEntity
- StatusConverters

### Telegram transport
- TelegramBotClient
- TelegramDocument / TelegramFile / TelegramMessage
- ChunkedUploadManager
- ChunkedDownloadManager
- StreamingChunkRequestBody
- UploadCancellationManager
- TokenRateLimiter
- Balancer / BalancerStats

### Repository/domain
- TelegramRepository
- LocalFileRepository
- DocumentMeta
- CloudFile
- UploadRequest / DownloadRequest

### Sharing
- ShareLinkManager
- MultiLinkGenerator
- MultiLinkDownloadManager
- LinkDownloadManager

### Synchronization
- SyncEngine
- SyncCrypto
- SyncLogManager
- SyncMetadataDao / SyncMetadataEntity
- SyncOperation / SyncOperationConverter
- ConflictResolver
- SyncConfig

### Gallery/media
- GallerySyncManager
- GalleryRestoreManager
- GalleryMediaDao / GalleryMediaEntity
- MultiFileGalleryManager
- ChunkedStreamingManager
- ChunkedVideoPlayer
- MediaScanner
- ThumbnailCache

### Tasks
- TaskQueue
- TaskQueueManager
- ProgressNotificationManager
- WorkManager-related workers

### UI
Jetpack Compose screens/components include dashboard, setup, wizard, gallery, task queue, theme, transitions and media viewer components.

## Database schema recovered from embedded SQL
The app has first-party tables including:
- `cloud_files`
- `upload_tasks`
- `download_tasks`
- `gallery_media`
- `sync_logs`
- `sync_metadata`

Important recovered columns include Telegram message/file identifiers, uploader bot tokens, chunk state, chunk file IDs, progress, checksums, target paths, gallery metadata and sync state.

The exact embedded CREATE/ALTER statements recovered as strings are in `dex/sql_schema_strings.txt`.

## Telegram API evidence
DEX strings explicitly contain:
- `https://api.telegram.org/bot`
- `https://api.telegram.org/file/bot`
- BotFather URL
- Telegram operations such as `sendDocument`, `getFile`, `sendChunk`, `getMessageFileId`, `forwardMessage`, `pinChatMessage`, `sendTextMessage`, and file download operations.

The application therefore implements Telegram Bot API access directly rather than merely embedding a Telegram client.

## Transfer architecture
The recovered class/method names show a resumable, database-backed transfer system:
- chunked uploads
- chunked downloads
- worker/job concurrency
- persisted completed chunk state
- cancellation
- progress tracking
- token balancing/rate limiting
- checksum handling
- streaming chunk bodies

The APK also contains explicit methods for resuming interrupted chunked uploads and downloads.

The user's observed ability to handle very large logical files is consistent with this architecture: the application can represent one logical file as many Telegram-backed chunks. This report does **not** assert an exact 10 GB hard limit unless it can be recovered from code/configuration.

## Backup and desktop interoperability
The Android APK contains a substantial BackupManager implementation, including:
- create/restore backup
- password-required restore paths
- encrypted backup import
- desktop database migration-related methods

Native strings include `TelegramCloudBackup`, `.env` handling, manifest handling and `BOT_TOKEN` / `CHANNEL_ID` validation. This is a strong fingerprint for the missing desktop project's data/configuration format.

## Obfuscation/recoverability assessment
The application's own package names, class names, field names and method names are unusually well preserved.
Examples include `ChunkedUploadManager`, `TelegramBotClient`, `ShareLinkManager`, `SyncEngine`, `BackupManager`, etc.

Therefore the Kotlin/Java layer should be substantially reconstructable from the APK.

The native layer is harder: exported JNI names and many symbols/strings survive, but no DWARF debug information was found. Native C++ can therefore be reconstructed through disassembly/decompilation, but it cannot be guaranteed to match the original source text.

## Exact artifact hashes
See `metadata/hashes.txt`.

## Important limitation
This is a forensic/reconstruction analysis, not the original Git repository. Decompiled/reconstructed source is not byte-for-byte equivalent to the author's deleted source. The supplied APK remains the authoritative artifact for this analysis.

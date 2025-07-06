help me develop a plan for implementing a DB-backed REST and queue worker backend for a user account system for a game matchmaking website. This is for a riichi mahjong game website that is focused on both player v. player matches as well as the development and play of bots against both humans and other bots. To NOT start writing code, only work on a markdown plan file.

The goal is this service handles migrations, RESTful serving, as well as a mode of operation for operating on queue messages meant to be consumed for database manipulation from other migration services. Therefore, I am imagining being able to start this service in a few different modes:

- Migration: Perform a migration
- Service: Attempt to start up (or wait for migrations to be complete) and then serve RESTful requests
- Worker: Consume incoming queue messages and perform database operations based on those messages
- Jobs: Run scheduled maintenance and cleanup jobs (designed for Kubernetes CronJobs)

# YOU ARE RUNNING IN A NIXOS ENVIRONMENT

A lot of commands will not be accessible to you. To run commands,

use `nix develop --command bash -c "<your command here>"`

# Layout of this project

- migration crate: where you will write SeaORM based migrations for the database entities
- entity crate: where you will generate entities for the database using SeaORM CLI operations.
- src: the source code for the service.

# User Security in mind

For the user, I don't want to require an email (I won't use it), but I do want to have a lot of security based things in mind:

- WebAuthn for 2-Factor Passkeys
- mCaptcha when signing up

# The Database Entities

Right now I have the following entities in mind to design:

## User

0. Id - Uuid of the player
1. Username - Alphanumeric, minimum of three characters
2. Country - Country Code, but I want to extend it by also supporting those for pride flags. Base it off of the iso3166 alpha 3, use this crate https://docs.rs/country-code/0.3.0/country_code/iso3166_1/alpha_3/index.html

to extend it, come up with additional codes that can be checked for pride flags (pick some sort of first character prefix, something not alpha based so it'll never collide, maybe an \_, and then use the two letters for the rest)

3. FavoriteTile - A favorite mahjong tile. Make this a string and stick to the following template
   <suit><number>
   Suit can be Sou, Pin, Man, Honor
   Number is a number between 1 and 9

4. Pronouns - String

5. MatchMakingRank - The player's MMR, which is a representation of their current personal performance score.

6. Password - argon2 based password hash and salt

7. CreatedAt - Timestamp of when the account was created

8. Passkey - If the user has two-factor webauthn enabled, the passkey credential.

9. Role - The role of the user on the site. Can be Disabled, Active, Moderator, or Admin.

10. Email - Optional, but asked in case moderators or admins need to contact you about anything.

11. LastActiveAt - Timestamp of last activity

12. AccountStatus - Enum: Active, Suspended, Banned (separate from Role)

13. Settings - JSON blob for user preferences (theme, notifications, etc.)

## Bot

Bots are owned by a player and are able to play mahjong on their own.

0. Id - Uuid of the bot.
1. Name - Name of the bot (should be alphanumeric, no spaces).
2. OwnerId - The uuid of the owner.
3. SourceCode - A URL to the source code for this bot (optional)
4. MatchMakingRank - The bot's MMR, which is a representation oftheir current performance score.
5. ApiKey - The current API key to registry with this bot
6. Live - Whether this bot is live and running right now
7. Icon - An Emoji Code to represent this bot. I say emoji code as I plan to use a custom expanded emoji set.
8. Description - Optional description of the bot's strategy or personality
9. Version - Version string of the bot implementation
10. LastHeartbeat - Timestamp of last heartbeat/health check from the bot
11. CreatedAt - Timestamp

## Game History

A table that contains a blob of the game history. We are undecided on the format of this at the moment, but I think a blob will be ideal since we will want to highly compress the information.

0. Game History Id - Uuid of the game history
1. HistoryBlob - Small blob of the history

## Match

A table of completed matches that contains both players and bot activity.

0. Match Id - Uuid of the match
1. Lobby Id - The uuid of the lobby they queued into
2. GameHistory Id - The uuid of the game history
3. StartedAt - Timestamp
4. CompletedAt - Timestamp

And then for up to four participant (make the fourth participant optional, to later support three player play)

N. Was participant N a bot or human
N + 1. participant uuid
N + 2. participant final score
N + 3. participant MMR delta

## Lobby Pools

A table of the available lobbies to queue into. For now, lobby types are going to be pretty limited, but we want to leave space in the future for different rulesets, etc. Only admins can create lobbys.

0. Lobby Id - Uuid of the lobby
1. Lobby Name - The title of the lobby
2. Lobby description - A short description of the lobby
3. Lobby Preset - An enum that describes the lobbies mode of play. For now, that will be `GeneralFourPlayer` which allows Players to queue in with both Players (and back fill them with Bots if not enough people are queuing right now), and `AllBotsFourPlayer` (which is a bot only queue)
4. Active - Whether the Lobby is active or not

## Report Table

A table of potential violations that moderators and admins need to investigate.

0. Report Id - Uuid of the report
1. Author Id - The player that submitted the report
2. AccusedIds - CSV of the uuids this is against. Prepended with h/ or b/ to represent if against a bot or human.
3. Match Id - The uuid of the match this is related to (optional).
4. Offense type - Enum of some presents
   OffensiveName
   CheatingOrCoersion
   ToxicBehaviour
   Other
5. Description - Optional Report Description
6. Status - Enum: Active, Dismissed, ActionTaken
7. ReportWriteUp - Concluding write up
8. CreatedAt - Timestamp
9. Severity - Enum: Low, Medium, High, Critical
10. ModReportAuthor - Uuid of the Moderator/Admin that handled this report.
11. ConcludedAt - Timestamp

## Queue/Matchmaking

A table to track active players and bots waiting for matches.

0. Queue Id - Uuid of the queue entry
1. Participant Type - Enum: Human, Bot
2. Participant Id - Uuid of the player or bot
3. Lobby Id - The uuid of the lobby they want to join
4. Preferred MMR Range - Optional MMR range they're willing to play against
5. JoinedAt - Timestamp when they joined the queue
6. Status - Enum: Waiting, InMatch, Cancelled

## User Sessions

A table to track login sessions for security purposes.

0. Session Id - Uuid of the session
1. User Id - Uuid of the user
2. Token Hash - Hashed session token
3. Device Info - Optional device/browser information
4. IP Address - IP address of the session
5. CreatedAt - Timestamp
6. ExpiresAt - Timestamp
7. LastActiveAt - Timestamp
8. Status - Enum: Active, Expired, Revoked

## User Statistics

Detailed statistics separate from the core User table.

0. User Id - Uuid of the user (primary key, references User)
1. Total Games - Count of games played
2. Wins - Count of first place finishes
3. Second Place - Count of second place finishes
4. Third Place - Count of third place finishes
5. Fourth Place - Count of fourth place finishes
6. Average Score - Average final score across all games
7. Peak MMR - Highest MMR achieved
8. Current Streak - Current win/loss streak
9. Last Game At - Timestamp of last completed game

## Friendship

Social connections between users.

0. Friendship Id - Uuid of the friendship
1. Requester Id - Uuid of the user who sent the friend request
2. Addressee Id - Uuid of the user who received the friend request
3. Status - Enum: Pending, Accepted, Blocked
4. CreatedAt - Timestamp
5. UpdatedAt - Timestamp

## Notifications

System messages and notifications for users.

0. Notification Id - Uuid of the notification
1. User Id - Uuid of the recipient user
2. Type - Enum: FriendRequest, MatchInvite, SystemMessage, AdminAnnouncement, ModeratorAction
3. Title - Notification title
4. Message - Notification content
5. Related Id - Optional uuid of related entity (friend request, match, etc.)
6. Read - Boolean indicating if notification was read
7. CreatedAt - Timestamp
8. ExpiresAt - Optional timestamp for auto-expiring notifications

## Audit Log

Track significant user actions for security and moderation.

0. Audit Id - Uuid of the audit entry
1. User Id - Uuid of the user who performed the action
2. Action Type - Enum: Login, Logout, PasswordChange, RoleChange, Suspension, BotCreated, BotDeleted, ProfileUpdate
3. Details - JSON blob with action-specific details
4. IP Address - IP address where action was performed
5. Moderator Id - Optional uuid of moderator who performed the action (for admin actions)
6. CreatedAt - Timestamp

## Private Rooms

Private rooms that users can create to play with friends or invite others.

0. Room Id - Uuid of the private room
1. Host Id - Uuid of the user who created the room
2. Room Name - Name of the private room
3. Room Code - Short alphanumeric code for easy sharing (e.g., "ABC123")
4. Password - Optional password to join the room
5. Max Players - Maximum number of players (3 or 4)
6. Allow Bots - Whether bots can be invited to fill empty slots
7. Invite Only - Whether only invited users can join or anyone with the code
8. Room Settings - JSON blob for game rules/settings specific to this room
9. Status - Enum: Open, InGame, Closed
10. CreatedAt - Timestamp
11. ExpiresAt - Optional timestamp when the room auto-closes

## Room Invitations

Invitations to private rooms.

0. Invitation Id - Uuid of the invitation
1. Room Id - Uuid of the private room
2. Inviter Id - Uuid of the user who sent the invitation
3. Invitee Id - Uuid of the user being invited
4. Status - Enum: Pending, Accepted, Declined, Expired
5. CreatedAt - Timestamp
6. ExpiresAt - Timestamp when invitation expires

## Room Participants

Current participants in private rooms.

0. Participation Id - Uuid of the participation record
1. Room Id - Uuid of the private room
2. Participant Type - Enum: Human, Bot
3. Participant Id - Uuid of the player or bot
4. Seat Position - Enum: East, South, West, North (optional until game starts)
5. Is Ready - Boolean indicating if participant is ready to start
6. JoinedAt - Timestamp

## Bot Statistics

Detailed statistics for bots, separate from the core Bot table.

0. Bot Id - Uuid of the bot (primary key, references Bot)
1. Total Games - Count of games played
2. Wins - Count of first place finishes
3. Second Place - Count of second place finishes
4. Third Place - Count of third place finishes
5. Fourth Place - Count of fourth place finishes
6. Average Score - Average final score across all games
7. Peak MMR - Highest MMR achieved
8. Current Streak - Current win/loss streak
9. Last Game At - Timestamp of last completed game
10. Uptime Percentage - Percentage of time bot was responsive when called

## System Configuration

System-wide settings and feature flags.

0. Config Id - Uuid of the configuration entry
1. Key - Configuration key (e.g., "maintenance_mode", "mmr_k_factor")
2. Value - Configuration value (JSON for complex values)
3. Description - Human-readable description of the setting
4. UpdatedBy - Uuid of admin who last updated this setting
5. UpdatedAt - Timestamp

## Data Export Requests

Track user data export requests for GDPR compliance.

0. Export Id - Uuid of the export request
1. User Id - Uuid of the requesting user
2. Export Type - Enum: UserData, MatchHistory, BotStatistics, FullExport
3. Status - Enum: Pending, Processing, Completed, Failed
4. File Path - Path to generated export file (when completed)
5. RequestedAt - Timestamp
6. CompletedAt - Timestamp
7. ExpiresAt - Timestamp when export file will be deleted (7 days after completion)

# Design Requirements

## Rate Limiting

- The API must serve X-Rate-Limit headers for all endpoints
- Rate limiting enforcement handled at ingress level, not application level
- Hard denial of requests exceeding limits

## Data Integrity & Deletion Policies

### Account Deletion

- When users delete accounts: anonymize all related data, disable login
- Preserve match history and statistics with anonymized user references
- Replace username with "Anonymous User {random_id}" in all records

### Soft Deletes

- Implement soft deletes for critical entities: Users, Matches, Reports, Audit Logs
- Use `deleted_at` timestamp field (NULL = active, timestamp = soft deleted)
- Soft deleted records excluded from normal queries but preserved for integrity

### Data Retention (GDPR Compliance)

- Audit logs: 60 days retention
- Expired sessions: 30 days retention
- Completed export files: 7 days retention
- Dismissed reports: 90 days retention
- User anonymization is permanent (no recovery)

## Database Indexes

Plan indexes for frequently queried fields:

- Users: username, email, role, account_status, last_active_at
- Bots: owner_id, live, mmr
- Matches: lobby_id, started_at, participant IDs
- Queue: lobby_id, status, joined_at
- Sessions: user_id, status, expires_at
- Notifications: user_id, read, created_at

## Jobs Service Mode

The Jobs service mode is designed to run scheduled maintenance and cleanup tasks via Kubernetes CronJobs. Jobs are invoked with a job type parameter:

```bash
# Example job invocations
./sth-account jobs cleanup-audit-logs
./sth-account jobs cleanup-sessions
./sth-account jobs cleanup-export-files
./sth-account jobs cleanup-reports
./sth-account jobs update-statistics
```

### Job Types

**Data Retention Jobs:**

- `cleanup-audit-logs`: Delete audit logs older than 60 days
- `cleanup-sessions`: Delete expired sessions older than 30 days
- `cleanup-export-files`: Delete export files older than 7 days
- `cleanup-reports`: Delete dismissed reports older than 90 days
- `cleanup-notifications`: Delete read notifications older than 30 days

**Maintenance Jobs:**

- `update-statistics`: Recalculate user and bot statistics (daily)
- `cleanup-stale-queues`: Remove abandoned queue entries (hourly)
- `cleanup-expired-rooms`: Close expired private rooms (hourly)
- `heartbeat-check`: Mark bots as offline if no heartbeat in 10 minutes (every 5 minutes)

**Analytics Jobs:**

- `generate-daily-stats`: Generate daily platform statistics
- `mmr-recalculation`: Periodic MMR adjustments if algorithm changes

### Kubernetes CronJob Examples

```yaml
# Cleanup expired sessions daily at 2 AM
apiVersion: batch/v1
kind: CronJob
metadata:
  name: cleanup-sessions
spec:
  schedule: "0 2 * * *"
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: sth-account
            image: sth-account:latest
            args: ["jobs", "cleanup-sessions"]
          restartPolicy: OnFailure

# Check bot heartbeats every 5 minutes
apiVersion: batch/v1
kind: CronJob
metadata:
  name: heartbeat-check
spec:
  schedule: "*/5 * * * *"
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: sth-account
            image: sth-account:latest
            args: ["jobs", "heartbeat-check"]
          restartPolicy: OnFailure
```

### Job Execution Features

- Each job logs execution start/completion with timestamps
- Failed jobs should log errors and exit with non-zero status
- Jobs are idempotent and safe to run multiple times
- Jobs respect soft-delete patterns (only hard-delete when appropriate)
- Long-running jobs should report progress periodically

# Technology Stack

## Core Dependencies

- **SeaORM**: Database operations, migrations, and entity management
- **amqp-rs**: RabbitMQ integration for Worker mode message processing
- **axum**: Web framework for REST API endpoints
- **tower**: Middleware ecosystem (sessions, auth, logging)
- **tokio**: Async runtime with full features
- **tracing + tracing-subscriber**: Structured logging and observability

## Authentication & Security

- **axum-login**: Session-based authentication middleware
- **tower-sessions**: Session management and storage
- **jsonwebtoken**: JWT token handling (if needed)
- **argon2**: Password hashing
- **uuid**: UUID generation for all entity IDs

## API Design

- **Versioning Strategy**: Route prefixes (`/v1/`, `/v2/`) with addition-only evolution
- **Response Format**: JSON with consistent error handling patterns
- **Headers**: X-Rate-Limit headers required on all endpoints
- **Health Checks**: `/health` and `/ready` endpoints for Kubernetes probes

## Development Environment

- **Nix**: Development shell with Rust toolchain and dependencies
- **Cargo Workspace**: Root crate + entity + migration modules
- **Rust Edition**: 2024 with toolchain version 1.88

## Deployment Architecture

- **Kubernetes**: CronJobs for scheduled tasks, Deployments for services
- **Container**: Single binary with multiple service modes
- **Configuration**: Environment variables for different environments
- **Database**: PostgreSQL with connection pooling

# Implementation Priority

## Phase 1: Foundation

1. Set up SeaORM entities from database design
2. Create service mode CLI structure (Migration, Service, Worker, Jobs)
3. Implement database migrations
4. Basic health check endpoints

## Phase 2: Core API

1. User authentication and session management
2. Basic CRUD operations for Users and Bots
3. Rate limiting headers implementation
4. Error handling patterns

## Phase 3: Game Features

1. Matchmaking queue system
2. Private room creation and management
3. Match history tracking
4. Statistics calculation

## Phase 4: Social & Moderation

1. Friend system and notifications
2. Reporting system
3. Admin/moderator tools
4. Data export functionality

## Phase 5: Maintenance

1. All scheduled jobs implementation
2. GDPR compliance features
3. Audit logging
4. Performance optimization

# Key Implementation Notes

- **Database Design**: All entities use UUIDs as primary keys
- **Soft Deletes**: Implement `deleted_at` timestamp pattern for Users, Matches, Reports, Audit Logs
- **Account Deletion**: Anonymization strategy, not hard deletion
- **Error Handling**: Consistent error responses, proper HTTP status codes
- **Logging**: Structured logs with request tracing
- **Testing**: Unit tests for business logic, integration tests for API endpoints
- **Security**: WebAuthn support for 2FA, mCaptcha integration for registration

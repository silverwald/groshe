# Groshe (Гроше)

Groshe is a local-first, peer-to-peer shared grocery list system designed for resilience, privacy, and zero infrastructure requirements.

## Key Features
- **Peer-to-Peer Synchronization**: Synchronizes directly over local network (mDNS) or encrypted Iroh relays with no central database.
- **Cross-Platform**: Android mobile app, headless always-on replica daemon, and an embedded Web Companion.
- **Real-Time Web Companion**: Lightweight web interface embedded directly into the daemon (`grocery serve --web`) with sub-100ms WebSocket synchronization, shorthand grammar parsing, and responsive Material 3 design.
- **CRDT Convergence**: Last-Write-Wins and fractional indexing for items, checkmarks, positions, and category grouping.

## CLI & Web Companion Usage

```bash
# Initialize local identity
grocery init

# Create a shared list
grocery list create "Weekly Groceries"

# Add items with inline shorthand grammar
grocery add "2 gal organic milk @Dairy"

# Check off an item
grocery check "organic milk"

# Launch always-on sync daemon with embedded Web Companion on port 8080
grocery serve --web 127.0.0.1:8080
```

Open `http://127.0.0.1:8080` in any desktop or mobile browser to view lists, add items with shorthand syntax, check off items, and view photo attachments in real time.

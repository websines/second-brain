# macOS Audio Setup Guide

Second Brain needs to capture system audio (Zoom, Teams, Meet) to transcribe remote participants. On macOS, this requires BlackHole - a free virtual audio driver.

## Quick Setup (5 minutes)

### 1. Install BlackHole

```bash
brew install blackhole-2ch
```

Or download from: https://existential.audio/blackhole/

### 2. Create Multi-Output Device

1. Open **Audio MIDI Setup** (press `Cmd+Space`, type "Audio MIDI Setup")
2. Click **+** at bottom-left → **Create Multi-Output Device**
3. Check both:
   - ☑ BlackHole 2ch
   - ☑ Your speakers/headphones
4. Right-click it → **Use This Device For Sound Output**

### 3. Done!

Your microphone stays as the input device. Second Brain will automatically detect BlackHole for system audio.

## How It Works

```
Meeting Audio → Multi-Output → Speakers (you hear)
                            → BlackHole (app captures)

Your Mic → App (captured separately)
```

## Verify Setup

When you start recording, check the console for:
```
Found loopback device: BlackHole 2ch
```

## Troubleshooting

| Issue | Solution |
|-------|----------|
| No system audio captured | Ensure Multi-Output Device is set as sound output |
| Can't hear meeting audio | Check that speakers are enabled in Multi-Output Device |
| BlackHole not detected | Restart the app after installing BlackHole |

## Without BlackHole

The app works without BlackHole - it will only capture your microphone. Remote participants won't be transcribed.

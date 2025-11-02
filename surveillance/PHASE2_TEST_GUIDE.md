# Phase 2 Manual Testing Guide

## Quick Test

Run the TUI with test video:
```bash
cd /home/rusty/cam_sys2/surveillance
cargo run --package gstreamed_ort -- video webcam_test.mp4 --tui
```

## Test Sequence

### 1. Enter Zone Management
- **Action**: Press `Z` while in monitor mode
- **Expected**: Screen switches to zone list view
- **Check**: Header shows "ROI Zone Management"

### 2. Create First Zone
- **Action**: Press `N`
- **Expected**: Enter zone editor with default zone (25%-75%)
- **Check**: Preview shows centered box

### 3. Adjust Zone Size
- **Action**: Press `→` several times
- **Expected**: Right edge moves right
- **Check**: Coordinates update in form, preview updates

- **Action**: Press `↓` several times
- **Expected**: Bottom edge moves down
- **Check**: Area percentage increases

### 4. Save Zone
- **Action**: Press `S`
- **Expected**: Return to zone list
- **Check**: New zone appears in table

### 5. Create Second Zone (Small, Top-Left)
- **Action**: Press `N`
- **Expected**: Enter editor again

- **Action**: Press `Ctrl+→` 3 times, `Ctrl+↓` 3 times
- **Expected**: Top-left corner moves to ~15%

- **Action**: Press `←` 5 times, `↑` 5 times
- **Expected**: Bottom-right shrinks to ~25%

- **Action**: Press `S`
- **Expected**: Small zone in top-left created

### 6. Toggle Zone Enable/Disable
- **Action**: Select first zone (↑/↓), press `Space`
- **Expected**: Status changes from ✓ to ✗ (green to red)

- **Action**: Press `Space` again
- **Expected**: Status returns to ✓ (green)

### 7. Edit Existing Zone
- **Action**: Press `E` on selected zone
- **Expected**: Enter editor with current coordinates

- **Action**: Adjust and save
- **Expected**: Changes persist

### 8. Delete Zone
- **Action**: Select zone 2 (↓), press `D`
- **Expected**: Zone removed from list
- **Check**: Only 1 zone remains

### 9. Return to Monitor
- **Action**: Press `Esc`
- **Expected**: Return to detection view
- **Check**: Footer shows "[Z] Zones" option

### 10. Verify Persistence
- **Action**: Press `Q` to quit

- **Action**: Run again: `cargo run --package gstreamed_ort -- video webcam_test.mp4 --tui`

- **Action**: Press `Z`
- **Expected**: Previously created zone(s) still exist
- **Check**: `zones.json` file exists in project root

## Expected UI Elements

### Zone List View:
```
┌─────────────────────────────────────────────────┐
│ ROI Zone Management | 2 zones                  │
├─────────────────────────────────────────────────┤
│ #  Name     Active Objects Area   Top-Left ... │
│ 1  Zone1    ✓      0       25.0%  (0.25,0.25)  │
│ 2  Zone2    ✗      0       6.25%  (0.10,0.10)  │
└─────────────────────────────────────────────────┘
```

### Zone Editor:
```
┌─────────────────┬─────────────────┐
│ Properties      │ Preview         │
│                 │                 │
│ Name: New Zone  │     ┌─────┐     │
│                 │     │     │     │
│ Top-Left:       │     │     │     │
│   X: 25.0%      │     └─────┘     │
│   Y: 25.0%      │                 │
│                 │                 │
│ Bottom-Right:   │                 │
│   X: 75.0%      │                 │
│   Y: 75.0%      │                 │
│                 │                 │
│ Area: 25.0%     │                 │
└─────────────────┴─────────────────┘
```

## Troubleshooting

### Zone preview doesn't show
- Check terminal size (needs at least 80x24)
- Verify coordinates are valid (0.0-1.0)

### Can't save zone
- Press `S` (capital or lowercase)
- Check terminal isn't capturing the key

### Zones don't persist
- Check file permissions in project directory
- Look for `zones.json` file
- Check for error messages on startup

## Success Criteria

- ✅ Can navigate between monitor and zone list
- ✅ Can create multiple zones
- ✅ Can edit zone coordinates
- ✅ Zone preview updates in real-time
- ✅ Zones persist to zones.json
- ✅ Zones load on startup
- ✅ Can toggle zones on/off
- ✅ Can delete zones
- ✅ UI is responsive and doesn't crash

#!/bin/bash
# Test which Alt+Arrow combinations work

echo "Testing Alt+Arrow key detection..."
echo ""
echo "This will test if your terminal passes Alt+Arrow keys correctly."
echo "Many terminals intercept Alt+Up/Down for scrolling."
echo ""
echo "Press Ctrl+C to exit"
echo ""

cargo run --release --bin gstreamed_ort -- webcam_test.mp4 2>/dev/null &
PID=$!

echo "TUI started (PID: $PID)"
echo ""
echo "Test in the Zone Editor:"
echo "1. Press 'Z' then 'N' to create a zone"
echo "2. Try each combination:"
echo "   - Alt+Left (should work)"
echo "   - Alt+Right (should work)"
echo "   - Alt+Up (may not work - terminal conflict)"
echo "   - Alt+Down (may not work - terminal conflict)"
echo ""
echo "If Alt+Up/Down don't work, we'll add alternative keys."
echo ""

wait $PID

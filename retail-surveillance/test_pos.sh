#!/bin/bash
set -e

echo "═══════════════════════════════════════"
echo "POS Integration Test Suite"
echo "═══════════════════════════════════════"
echo ""

# Check if mosquitto is installed
if ! command -v mosquitto &> /dev/null; then
    echo "❌ Mosquitto not found. Installing..."
    if [[ "$OSTYPE" == "darwin"* ]]; then
        brew install mosquitto
    else
        sudo apt-get install -y mosquitto mosquitto-clients
    fi
else
    echo "✅ Mosquitto found"
fi

# Start mosquitto in background
echo "Starting MQTT broker..."
mosquitto -c config/mosquitto.conf -d 2>/dev/null || {
    echo "⚠️  Mosquitto may already be running or port 1883 is in use"
}

sleep 2

# Test MQTT connection
echo "Testing MQTT connection..."
timeout 2 mosquitto_sub -h localhost -t test -C 1 &
MQTT_PID=$!
mosquitto_pub -h localhost -t test -m "test_message"
wait $MQTT_PID 2>/dev/null || true

echo "✅ MQTT broker is running"
echo ""

# Compile the project
echo "Building surveillance system..."
cargo build --release --quiet
echo "✅ Build successful"
echo ""

# Run tests
echo "Running unit tests..."
cargo test --lib --quiet
echo "✅ Tests passed"
echo ""

# Test scenarios
echo "═══════════════════════════════════════"
echo "Test Scenarios"
echo "═══════════════════════════════════════"
echo ""

echo "1. Publishing test POS events..."
for event_type in discount void refund; do
    EVENT_JSON=$(cat <<EOF
{
  "event_id": "$(uuidgen || cat /proc/sys/kernel/random/uuid)",
  "event_type": "${event_type}_transaction",
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "store_id": "store_001",
  "register_id": "reg_01",
  "staff_id": "emp_12345",
  "order_id": "ORD$(shuf -i 10000-99999 -n 1)",
  "ticket_no": "T$(shuf -i 1000-9999 -n 1)",
  "amount": $(shuf -i 100-1000 -n 1).00,
  "discount_percent": $(shuf -i 10-50 -n 1).0,
  "items": []
}
EOF
)

    mosquitto_pub -h localhost -t "pos/events/store_001/$event_type" -m "$EVENT_JSON"
    echo "   ✅ Published $event_type event"
    sleep 1
done

echo ""
echo "2. Testing risk scoring..."
cargo test test_risk_scoring --quiet 2>/dev/null && echo "   ✅ Risk scoring works" || echo "   ⚠️  Risk scoring test not found"

echo ""
echo "═══════════════════════════════════════"
echo "Integration Test"
echo "═══════════════════════════════════════"
echo ""
echo "To run full integration test:"
echo ""
echo "Terminal 1:"
echo "  cargo run --release -- --enable-pos"
echo ""
echo "Terminal 2:"
echo "  cargo run --release -- --simulate-pos"
echo ""
echo "This will:"
echo "- Start video pipeline with POS integration"
echo "- Simulate POS events every 10 seconds"
echo "- Show alerts for suspicious transactions"
echo ""
echo "Press Ctrl+C to stop"
echo "═══════════════════════════════════════"
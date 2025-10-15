#!/bin/bash

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║          POS Integration Live Demo                             ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Check prerequisites
command -v mosquitto_pub >/dev/null 2>&1 || {
    echo "❌ mosquitto_pub not found. Please install: brew install mosquitto"
    exit 1
}

echo "✅ Prerequisites check passed"
echo ""

# Start MQTT broker if not running
if ! pgrep -x "mosquitto" > /dev/null; then
    echo "Starting MQTT broker..."
    /opt/homebrew/opt/mosquitto/sbin/mosquitto -c config/mosquitto.conf -d 2>/dev/null || echo "⚠️  Mosquitto may already be running"
    sleep 2
    echo "✅ MQTT broker started"
else
    echo "✅ MQTT broker already running"
fi
echo ""

# Scenario functions
send_event() {
    local event_type=$1
    local staff_id=$2
    local amount=$3
    local discount=$4
    local description=$5

    ORDER_ID="ORD$(shuf -i 10000-99999 -n 1 2>/dev/null || echo $RANDOM)"
    TICKET_NO="T$(shuf -i 1000-9999 -n 1 2>/dev/null || echo $RANDOM)"

    EVENT_JSON=$(cat <<EOF
{
  "event_id": "$(uuidgen 2>/dev/null || cat /proc/sys/kernel/random/uuid 2>/dev/null || echo "test-$RANDOM")",
  "event_type": "${event_type}",
  "timestamp": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "store_id": "store_001",
  "register_id": "reg_02",
  "staff_id": "${staff_id}",
  "order_id": "${ORDER_ID}",
  "ticket_no": "${TICKET_NO}",
  "amount": ${amount},
  "original_amount": $(echo "$amount / (1 - $discount / 100)" | bc 2>/dev/null || echo "$amount"),
  "discount_percent": ${discount},
  "items": [
    {
      "sku": "ELEC001",
      "name": "Electronics Item",
      "quantity": 1,
      "unit_price": ${amount},
      "total_price": ${amount}
    }
  ],
  "metadata": {}
}
EOF
)

    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "📋 Scenario: $description"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "Event Type:  $event_type"
    echo "Employee:    $staff_id"
    echo "Order:       $ORDER_ID"
    echo "Ticket:      $TICKET_NO"
    echo "Amount:      \$$amount"
    echo "Discount:    ${discount}%"
    echo ""

    # Publish to MQTT
    mosquitto_pub -h localhost -t "pos/events/store_001/${event_type}" -m "$EVENT_JSON"

    echo "✅ Event published to MQTT"
    echo ""
    sleep 2
}

# Demo scenarios
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                    Demo Scenarios                              ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "This demo will simulate 5 real-world POS scenarios:"
echo ""
echo "1. 🟢 Normal transaction (Low risk)"
echo "2. 🟡 Large discount (Medium risk)"
echo "3. 🟠 Void transaction (Medium-high risk)"
echo "4. 🔴 High-value refund (High risk)"
echo "5. 🔴 Suspicious pattern (Very high risk)"
echo ""
read -p "Press Enter to start demo..."
echo ""

# Scenario 1: Normal
send_event \
    "payment_cleared" \
    "emp_54321" \
    "45.99" \
    "0" \
    "Normal checkout - legitimate purchase"

sleep 3

# Scenario 2: Moderate discount
send_event \
    "discount_applied" \
    "emp_12345" \
    "120.00" \
    "35" \
    "Large discount applied - requires review"

sleep 3

# Scenario 3: Void
send_event \
    "void_transaction" \
    "emp_12345" \
    "89.99" \
    "0" \
    "Transaction voided - customer changed mind?"

sleep 3

# Scenario 4: High-value refund
send_event \
    "refund_issued" \
    "emp_12345" \
    "1250.00" \
    "0" \
    "High-value refund without receipt"

sleep 3

# Scenario 5: Suspicious pattern
send_event \
    "discount_applied" \
    "emp_12345" \
    "450.00" \
    "50" \
    "50% discount on electronics - 3rd time today!"

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                    Demo Complete                               ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "Summary of events published:"
echo ""
echo "Event 1: Normal payment (Low risk)          → No alert"
echo "Event 2: 35% discount (Medium risk)         → Alert triggered"
echo "Event 3: Void transaction (Medium-high)     → Alert triggered"
echo "Event 4: $1250 refund (High risk)           → Alert triggered"
echo "Event 5: 50% discount (Very high risk)      → URGENT alert"
echo ""
echo "Employee 'emp_12345' shows suspicious pattern:"
echo "  - 3 high-risk events in short time"
echo "  - Pattern suggests possible theft"
echo "  - Video review recommended"
echo ""
echo "To view events in real-time, run in another terminal:"
echo "  cargo run --release -- --enable-pos"
echo ""
echo "To monitor MQTT messages:"
echo "  mosquitto_sub -h localhost -t 'pos/events/#' -v"
echo ""
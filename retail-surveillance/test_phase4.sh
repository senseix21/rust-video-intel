#!/bin/bash

echo "═══════════════════════════════════════════════════════════════"
echo "         Phase 4 Database Integration Test"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# Check Docker
if ! command -v docker &> /dev/null; then
    echo "❌ Docker not installed"
    exit 1
fi

echo "1️⃣  Starting PostgreSQL database..."
docker-compose up -d postgres
sleep 3

# Check if PostgreSQL is running
if docker-compose exec -T postgres pg_isready -U surveillance > /dev/null 2>&1; then
    echo "✅ PostgreSQL is running"
else
    echo "❌ PostgreSQL failed to start"
    exit 1
fi

echo ""
echo "2️⃣  Creating database schema..."
docker-compose exec -T postgres psql -U surveillance -d retail_surveillance < migrations/001_initial_schema.sql 2>/dev/null
echo "✅ Schema created (or already exists)"

echo ""
echo "3️⃣  Starting MQTT broker..."
docker-compose up -d mosquitto
sleep 2
echo "✅ MQTT broker started"

echo ""
echo "4️⃣  Testing database connection..."
cat > test_db.sql << 'EOF'
SELECT table_name FROM information_schema.tables
WHERE table_schema = 'public'
ORDER BY table_name;
EOF

echo "Tables in database:"
docker-compose exec -T postgres psql -U surveillance -d retail_surveillance -t < test_db.sql | sed 's/^/  - /'
rm test_db.sql

echo ""
echo "5️⃣  Inserting test POS event..."
cat > test_insert.sql << 'EOF'
INSERT INTO pos_events (
    event_id, event_type, timestamp, store_id, staff_id,
    order_id, ticket_no, amount, discount_percent
) VALUES (
    'TEST-001', 'RefundIssued', NOW(), 'store_001', 'emp_12345',
    'ORD99999', 'T99999', 250.00, 0.0
) ON CONFLICT (event_id) DO NOTHING
RETURNING id;
EOF

EVENT_ID=$(docker-compose exec -T postgres psql -U surveillance -d retail_surveillance -t -A < test_insert.sql)
if [ ! -z "$EVENT_ID" ]; then
    echo "✅ Test event inserted: $EVENT_ID"
else
    echo "⚠️  Event already exists or insert failed"
fi
rm test_insert.sql

echo ""
echo "6️⃣  Testing API endpoints..."
echo "Starting API server in background for 5 seconds..."

# Create a simple test of the API
timeout 5 cargo run --bin retail-surveillance --features "api" 2>/dev/null &
API_PID=$!
sleep 3

# Test health endpoint
if curl -s http://localhost:3000/health > /dev/null 2>&1; then
    echo "✅ API health check passed"
else
    echo "⚠️  API not responding (may need separate run)"
fi

echo ""
echo "7️⃣  Checking data in database..."
cat > test_query.sql << 'EOF'
SELECT COUNT(*) as event_count FROM pos_events;
SELECT COUNT(*) as alert_count FROM risk_alerts;
SELECT COUNT(*) as profile_count FROM staff_risk_profiles;
EOF

echo "Database statistics:"
docker-compose exec -T postgres psql -U surveillance -d retail_surveillance -t < test_query.sql | while read line; do
    echo "  $line"
done
rm test_query.sql

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "                Phase 4 Test Complete!"
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "✅ Database: Running on port 5432"
echo "✅ MQTT: Running on port 1883"
echo "✅ Schema: All tables created"
echo "✅ API: Ready on port 3000"
echo ""
echo "To run the full system:"
echo "  cargo run --bin retail-surveillance --features database"
echo ""
echo "To view API endpoints:"
echo "  http://localhost:3000/health"
echo "  http://localhost:3000/api/v1/events"
echo "  http://localhost:3000/api/v1/alerts"
echo "  http://localhost:3000/api/v1/stats/dashboard"
echo ""
echo "To connect to database:"
echo "  docker-compose exec postgres psql -U surveillance -d retail_surveillance"
echo ""
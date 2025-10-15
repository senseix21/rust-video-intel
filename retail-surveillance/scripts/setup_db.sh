#!/bin/bash

echo "═══════════════════════════════════════════════════════════════"
echo "         Setting up PostgreSQL Database for Phase 4"
echo "═══════════════════════════════════════════════════════════════"
echo ""

# Start PostgreSQL container
echo "🚀 Starting PostgreSQL container..."
docker-compose up -d postgres

# Wait for PostgreSQL to be ready
echo "⏳ Waiting for PostgreSQL to be ready..."
for i in {1..30}; do
    if docker-compose exec -T postgres pg_isready -U surveillance > /dev/null 2>&1; then
        echo "✅ PostgreSQL is ready!"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "❌ PostgreSQL failed to start"
        exit 1
    fi
    sleep 1
done

# Run migrations
echo ""
echo "📝 Running database migrations..."
docker-compose exec -T postgres psql -U surveillance -d retail_surveillance < migrations/001_initial_schema.sql

if [ $? -eq 0 ]; then
    echo "✅ Database schema created successfully!"
else
    echo "❌ Failed to create database schema"
    exit 1
fi

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "                Database Setup Complete!"
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "Connection string:"
echo "  postgres://surveillance:secure_password@localhost:5432/retail_surveillance"
echo ""
echo "To connect with psql:"
echo "  docker-compose exec postgres psql -U surveillance -d retail_surveillance"
echo ""
echo "To stop the database:"
echo "  docker-compose down"
echo ""
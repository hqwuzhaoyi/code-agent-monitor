#!/bin/bash
# Restart openclaw gateway

set -e

echo "Restarting openclaw gateway..."
openclaw gateway restart

echo "Gateway restarted successfully"

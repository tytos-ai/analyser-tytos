#!/bin/bash

echo "=== All Environment Variables Starting with PNL ==="
export PNL__BIRDEYE__API_KEY="5ff313b239ac42e297b830b10ea1871d"

# Print all environment variables that start with PNL
env | grep ^PNL | sort

echo
echo "=== Testing different variable name formats ==="

# Test different variations
export PNL_BIRDEYE_API_KEY="5ff313b239ac42e297b830b10ea1871d"
echo "With single underscore: PNL_BIRDEYE_API_KEY='$PNL_BIRDEYE_API_KEY'"

export PNL__BIRDEYE__API_KEY="5ff313b239ac42e297b830b10ea1871d"  
echo "With double underscore: PNL__BIRDEYE__API_KEY='$PNL__BIRDEYE__API_KEY'"

# Test what the config crate expects based on the field name
echo
echo "=== Checking field mapping ==="
echo "Config field: birdeye.api_key"
echo "Expected env var with prefix 'PNL' and separator '__': PNL__BIRDEYE__API_KEY"
#!/bin/bash
# Setup script for VHS demo
# Run this before: vhs demo.tape

set -e

rm -rf /tmp/gah-demo
mkdir -p /tmp/gah-demo
cd /tmp/gah-demo

git init -q
git config user.email 'demo@example.com'
git config user.name 'Demo'

# Initial file
cat > app.py << 'EOF'
def greet(name):
    return f"Hello, {name}!"

def add(a, b):
    return a + b

def main():
    print(greet("World"))
    print(add(2, 3))

if __name__ == "__main__":
    main()
EOF

git add app.py
git commit -q -m 'Initial commit'

# Make changes (multiple hunks)
cat > app.py << 'EOF'
def greet(name):
    # TODO: add greeting customization
    return f"Hello, {name}!"

def add(a, b):
    return a + b

def subtract(a, b):
    return a - b

def main():
    print(greet("World"))
    print(add(2, 3))
    print(subtract(5, 2))

if __name__ == "__main__":
    main()
EOF

echo "Demo repo ready at /tmp/gah-demo"
echo "Run: vhs demo.tape"

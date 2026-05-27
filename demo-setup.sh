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

# Initial file (longer to ensure changes create separate hunks)
cat > app.py << 'EOF'
"""Simple calculator application."""


def greet(name):
    """Return a greeting message."""
    return f"Hello, {name}!"


def add(a, b):
    """Add two numbers."""
    return a + b


def multiply(a, b):
    """Multiply two numbers."""
    return a * b


def divide(a, b):
    """Divide two numbers."""
    if b == 0:
        raise ValueError("Cannot divide by zero")
    return a / b


def main():
    """Run the calculator demo."""
    print(greet("World"))
    print(f"2 + 3 = {add(2, 3)}")
    print(f"4 * 5 = {multiply(4, 5)}")
    print(f"10 / 2 = {divide(10, 2)}")


if __name__ == "__main__":
    main()
EOF

git add app.py
git commit -q -m 'Initial commit'

# Make changes (spread apart to create multiple hunks)
cat > app.py << 'EOF'
"""Simple calculator application."""


def greet(name):
    # TODO: add greeting customization
    """Return a greeting message."""
    return f"Hello, {name}!"


def add(a, b):
    """Add two numbers."""
    return a + b


def subtract(a, b):
    """Subtract two numbers."""
    return a - b


def multiply(a, b):
    """Multiply two numbers."""
    return a * b


def divide(a, b):
    """Divide two numbers."""
    if b == 0:
        raise ValueError("Cannot divide by zero")
    return a / b


def main():
    """Run the calculator demo."""
    print(greet("World"))
    print(f"2 + 3 = {add(2, 3)}")
    print(f"4 - 1 = {subtract(4, 1)}")
    print(f"4 * 5 = {multiply(4, 5)}")
    print(f"10 / 2 = {divide(10, 2)}")


if __name__ == "__main__":
    main()
EOF

echo "Demo repo ready at /tmp/gah-demo"
echo "Run: vhs demo.tape"

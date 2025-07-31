#!/bin/bash
# Prepare release with prebuilds from GitHub Actions

set -e

echo "🚀 Native Vector Store Release Preparation"
echo "========================================"

# Check if we have a clean working directory
if [ -n "$(git status --porcelain)" ]; then
    echo "❌ Error: Working directory is not clean. Commit or stash changes first."
    exit 1
fi

# Get current version
VERSION=$(node -p "require('./package.json').version")
echo "📦 Current version: $VERSION"

# Check if gh CLI is installed
if ! command -v gh &> /dev/null; then
    echo "❌ Error: GitHub CLI (gh) is not installed. Install it with: brew install gh"
    exit 1
fi

echo ""
echo "📋 Steps to release version $VERSION:"
echo "1. Push a tag to trigger the build workflow:"
echo "   git tag v$VERSION"
echo "   git push origin v$VERSION"
echo ""
echo "2. Wait for the GitHub Actions workflow to complete:"
echo "   gh run watch"
echo ""
echo "3. Download the prebuilds artifact:"
echo "   gh run download --name prebuilds-linux-x64"
echo "   gh run download --name prebuilds-linux-arm64"
echo "   gh run download --name prebuilds-alpine-x64"
echo "   gh run download --name prebuilds-darwin-x64"
echo "   gh run download --name prebuilds-darwin-arm64"
echo "   gh run download --name prebuilds-win32-x64"
echo ""
echo "4. The artifacts will be in the prebuilds/ directory"
echo ""
echo "5. Publish to npm:"
echo "   npm publish"
echo ""
echo "Alternative: Use the npm-publish workflow with manual trigger"
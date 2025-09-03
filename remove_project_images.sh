#!/bin/bash

# Script to remove Docker images for moment project repositories one at a time
# ECR Repository: ***.dkr.ecr.ap-northeast-1.amazonaws.com/

set -e

# Read ECR_PREFIX from environment variable
if [ -z "$ECR_PREFIX" ]; then
    echo "❌ Error: ECR_PREFIX environment variable is not set"
    echo "   Please set ECR_PREFIX"
    exit 1
fi

echo "🔍 Searching for Docker images with ECR prefix: $ECR_PREFIX"

# Get all unique project repositories
PROJECTS=$(docker images --format "{{.Repository}}" | grep "$ECR_PREFIX" | sort | uniq)

if [ -z "$PROJECTS" ]; then
    echo "✅ No images found with ECR prefix: $ECR_PREFIX"
    exit 0
fi

echo "📋 Found the following project repositories:"
echo "$PROJECTS"
echo ""

# Function to remove images for a specific project
remove_project_images() {
    local project=$1
    echo "🔍 Checking images for project: $project"
    
    # Get images for this specific project
    local images=$(docker images --format "{{.Repository}}:{{.Tag}}" | grep "^$project")
    
    if [ -z "$images" ]; then
        echo "   ℹ️  No images found for $project"
        return 0
    fi
    
    echo "   📋 Found images:"
    echo "$images" | sed 's/^/      /'
    echo ""
    
    # Count images
    local count=$(echo "$images" | wc -l)
    echo "   📊 Total images for $project: $count"
    
    # Ask for confirmation
    read -p "   ⚠️  Remove all images for $project? (y/N): " -n 1 -r
    echo ""
    
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "   ❌ Skipping $project"
        return 0
    fi
    
    echo "   🗑️  Removing images for $project..."
    
    # Remove images for this project
    docker rmi -f $(docker images "$project" -q)
    
    echo "   ✅ Successfully removed images for $project"
    echo ""
}

# Process each project
for project in $PROJECTS; do
    remove_project_images "$project"
done

echo "🔍 Final verification..."
REMAINING=$(docker images --format "{{.Repository}}" | grep "$ECR_PREFIX" | sort | uniq)

if [ -z "$REMAINING" ]; then
    echo "✅ All project images successfully removed!"
else
    echo "📋 Remaining project repositories:"
    echo "$REMAINING"
fi

echo ""
echo "🧹 Running Docker system prune to clean up dangling images..."
docker system prune -f

echo "🎉 Cleanup completed!"

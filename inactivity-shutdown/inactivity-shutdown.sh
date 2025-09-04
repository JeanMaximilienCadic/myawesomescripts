#!/bin/bash

# Configuration
INACTIVITY_THRESHOLD=3600  # 1 hour in seconds
LOG_FILE="/var/log/inactivity-shutdown.log"
STATE_FILE="/var/lib/inactivity-shutdown.state"

# Function to log messages
log_message() {
    echo "$(date '+%Y-%m-%d %H:%M:%S') - $1" | tee -a "$LOG_FILE"
}

# Function to check if user is active
check_user_activity() {
    # Check for active SSH sessions
    if who | grep -q pts; then
        return 0  # User is active
    fi
    
    # Check for active GUI sessions (if X11 is running)
    if pgrep -x "Xorg\|wayland" > /dev/null; then
        # Check for mouse/keyboard activity in the last 5 minutes
        if [ -f /dev/input/mice ] || [ -d /dev/input/by-path ]; then
            # Look for recent input activity
            for input_dev in /dev/input/event*; do
                if [ -c "$input_dev" ]; then
                    # Check if device was accessed recently
                    if [ $(($(date +%s) - $(stat -c %Y "$input_dev" 2>/dev/null || echo 0))) -lt 300 ]; then
                        return 0  # User is active
                    fi
                fi
            done
        fi
    fi
    
    # Check for active processes that indicate user activity
    if pgrep -f "firefox\|chrome\|code\|vim\|nano\|emacs\|gedit" > /dev/null; then
        return 0  # User is active
    fi
    
    # Check for network activity (optional - might be too aggressive)
    # if ss -tuln | grep -q ":22\|:3389\|:5900"; then
    #     return 0  # User is active
    # fi
    
    return 1  # User is inactive
}

# Function to get current timestamp
get_timestamp() {
    date +%s
}

# Function to read last activity timestamp
get_last_activity() {
    if [ -f "$STATE_FILE" ]; then
        cat "$STATE_FILE"
    else
        echo "0"
    fi
}

# Function to update last activity timestamp
update_last_activity() {
    get_timestamp > "$STATE_FILE"
}

# Main logic
main() {
    log_message "Checking for user activity..."
    
    if check_user_activity; then
        # User is active, update timestamp
        update_last_activity
        log_message "User activity detected. Resetting inactivity timer."
    else
        # User is inactive, check how long
        last_activity=$(get_last_activity)
        current_time=$(get_timestamp)
        inactive_time=$((current_time - last_activity))
        
        log_message "No user activity detected. Inactive for $inactive_time seconds."
        
        if [ $inactive_time -ge $INACTIVITY_THRESHOLD ]; then
            log_message "Inactivity threshold reached ($INACTIVITY_THRESHOLD seconds). Initiating shutdown in 60 seconds..."
            
            # Notify users before shutdown (if possible)
            if command -v wall > /dev/null; then
                echo "System will shutdown in 60 seconds due to inactivity. Press Ctrl+C to cancel." | wall
            fi
            
            # Wait 60 seconds before shutdown
            sleep 60
            
            # Final check - if user became active during the 60-second warning, cancel shutdown
            if check_user_activity; then
                log_message "User activity detected during shutdown warning. Cancelling shutdown."
                exit 0
            fi
            
            log_message "Proceeding with shutdown due to inactivity."
            shutdown -h now "System shutdown due to inactivity"
        else
            remaining=$((INACTIVITY_THRESHOLD - inactive_time))
            log_message "Inactivity timer: $remaining seconds remaining until shutdown."
        fi
    fi
}

# Create necessary directories and files
mkdir -p "$(dirname "$LOG_FILE")"
mkdir -p "$(dirname "$STATE_FILE")"

# Initialize state file if it doesn't exist
if [ ! -f "$STATE_FILE" ]; then
    update_last_activity
fi

# Run main function
main

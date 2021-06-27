set -e

cargo objcopy --release -- -O ihex o.hex
echo "Now press the button..."
teensy_loader_cli --mcu=TEENSY40 -w ./o.hex 

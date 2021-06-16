cargo objcopy --release -- -O ihex o.hex
teensy_loader_cli --mcu=TEENSY40 -w ./o.hex 

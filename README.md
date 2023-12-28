### Touch switch

This project utilises the Raspberry Pi Pico to implement a touch controlled LED for a bedside ambient light.

The intention was to have a gradual fade on & off to avoid a sudden light level change.

This uses the pico's PIO to detect the touch, and the PIO code was borrowed from the python [jtouch](https://github.com/AncientJames/jtouch/tree/main) project.
From that base the rest of the pico code (in Rust) translates the touch status into long or short touches where a short touch fades the light on or off and a long
touch does an immediate on/off.

The LED is a smart LED of type APA102 and is controlled using the pico's SPI. This was chosen to enable a bright enough light without having to worry about a separate
LED driver for supplying enough current. Also, being a smart LED, more can be chained together if a single LED is not bright enough.

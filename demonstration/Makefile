CC      = mpicc
CFLAGS  = -Wall -Wextra -O2 -std=c11
TARGET  = demonstration

all: $(TARGET)

$(TARGET): main.c
	$(CC) $(CFLAGS) -o $(TARGET) main.c

clean:
	rm -f $(TARGET)

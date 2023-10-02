import struct 

ssid = 'Buffalo-G-1337'.encode('utf-8')
password = 'mysecretpw'.encode('utf-8')

assert (len(ssid) <= 32)
assert (len(password) <= 32)

base_address = 0x10fff000

def gen_block(address, block_id, data):
    assert(len(data) == 256)

    f = b''
    # magic number
    f += b'\x55\x46\x32\x0a'
    # magic number
    f += b'\x57\x51\x5d\x9e'
    # flags (familyID present)
    f += b'\x00\x20\x00\x00'
    # address where it should be written
    f += struct.pack('<i', address)
    # size of block (256)
    f += b'\x00\x01\x00\x00'
    # sequential block number
    f += struct.pack('<i', block_id)
    # total number of blocks
    f += struct.pack('<i', 16)
    # familyID
    f += b'\x56\xff\x8b\xe4'
    f += data
    # padding to bring block to 512 bytes
    f += (476 - 256) * b'\0'
    # magic number
    f += b'\x30\x6f\xb1\x0a'

    return f


wifi_data = ssid + (32 - len(ssid)) * b'\0'
wifi_data += password + (32 - len(password)) * b'\0'
wifi_data +=  (256 - 64) * b'\0'

file = b''
file += gen_block(base_address, 0, wifi_data)

for i in range(1, 16):
    file += gen_block(base_address + 256 * i, i, 256 * b'\0')

with open('./wifi.uf2', 'wb') as f:
    f.write(file)
import sys
try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    import subprocess
    subprocess.check_call([sys.executable, "-m", "pip", "install", "Pillow"])
    from PIL import Image, ImageDraw, ImageFont

width, height = 1024, 1024
cols, rows = 8, 8
cell_w = width // cols
cell_h = height // rows

# Create an image with RGBA
img = Image.new('RGBA', (width, height))
draw = ImageDraw.Draw(img)

# Gradient colors
top_left = (255, 0, 0)         # Red
top_right = (128, 0, 128)      # Purple
bottom_left = (200, 162, 200)  # Lila (Lilac)
bottom_right = (0, 0, 255)     # Blue

# Generate Gradient
pixels = img.load()
for y in range(height):
    ty = y / (height - 1)
    for x in range(width):
        tx = x / (width - 1)
        
        # Bilinear interpolation
        w00 = (1 - tx) * (1 - ty)
        w10 = tx * (1 - ty)
        w01 = (1 - tx) * ty
        w11 = tx * ty
        
        r = int(top_left[0]*w00 + top_right[0]*w10 + bottom_left[0]*w01 + bottom_right[0]*w11)
        g = int(top_left[1]*w00 + top_right[1]*w10 + bottom_left[1]*w01 + bottom_right[1]*w11)
        b = int(top_left[2]*w00 + top_right[2]*w10 + bottom_left[2]*w01 + bottom_right[2]*w11)
        
        pixels[x, y] = (r, g, b, 128) # 128 alpha for semi-transparent

# Draw Grid Lines
line_color = (255, 255, 255, 200)
for i in range(cols + 1):
    x = i * cell_w
    # keep inside bounds
    if x == width: x -= 1
    draw.line([(x, 0), (x, height)], fill=line_color, width=4)
for i in range(rows + 1):
    y = i * cell_h
    if y == height: y -= 1
    draw.line([(0, y), (width, y)], fill=line_color, width=4)

# Try loading a font
try:
    font = ImageFont.truetype("arial.ttf", 64)
except:
    font = ImageFont.load_default()

# Draw Text
letters = "ABCDEFGH"
for r in range(rows):
    for c in range(cols):
        text = f"{letters[c]}{r+1}"
        # Center text
        bbox = draw.textbbox((0, 0), text, font=font)
        tw = bbox[2] - bbox[0]
        th = bbox[3] - bbox[1]
        
        x = c * cell_w + (cell_w - tw) / 2
        y = r * cell_h + (cell_h - th) / 2
        
        draw.text((x, y), text, fill=(255, 255, 255, 255), font=font)

img.save("uv_grid.png")
print("Saved uv_grid.png successfully.")

AICAD 1
PART mounting_plate "Mounting Plate"
PARAM plate_width = 120
PARAM plate_height = 80
PARAM edge_offset = 15
PARAM hole_diameter := plate_height / 8
BASE_RECT main
  WIDTH 120
  HEIGHT 80
THICKNESS 10
BIND main WIDTH <- plate_width
BIND main HEIGHT <- plate_height
HOLE_PATTERN mounting CORNER
  BASE main
  OFFSET_X 15
  OFFSET_Y 15
  DIA 10
BIND mounting OFFSET_X <- edge_offset
BIND mounting OFFSET_Y <- edge_offset
DRAWING PROJECTION_METHOD THIRD_ANGLE
BIND mounting DIAMETER <- hole_diameter
EXTENT mounting THRU

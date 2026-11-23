cmake_minimum_required(VERSION 3.21)
project(brick VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release)
endif()

find_package(Qt6 6.2 REQUIRED COMPONENTS Widgets Network WebEngineWidgets)

# The Rust engine is built by cargo and linked as a static library.
set(BRICK_ROOT ${CMAKE_CURRENT_SOURCE_DIR}/..)
set(BRICK_CORE_LIB ${BRICK_ROOT}/target/release/libbrick_core.a)
add_custom_target(brick_core_build ALL
  COMMAND cargo build --release -p brick-core
  WORKING_DIRECTORY ${BRICK_ROOT}
  BYPRODUCTS ${BRICK_CORE_LIB}
  COMMENT "Building the Rust mail engine"
  USES_TERMINAL)

add_executable(brick
  src/main.cpp
  src/Core.cpp        src/Core.h
  src/MessageModel.cpp src/MessageModel.h
  src/MessageView.cpp  src/MessageView.h
  src/MainWindow.cpp   src/MainWindow.h
  src/ComposeWindow.cpp src/ComposeWindow.h
  src/AccountDialog.cpp src/AccountDialog.h
  src/AccountOptionsDialog.cpp src/AccountOptionsDialog.h
  src/Item.cpp         src/Item.h
  src/ItemStore.cpp    src/ItemStore.h
  src/ItemList.cpp     src/ItemList.h
  src/ItemEditor.cpp   src/ItemEditor.h
  src/ClaudeDialogs.cpp src/ClaudeDialogs.h
  src/TodoView.cpp     src/TodoView.h
  src/CalendarView.cpp src/CalendarView.h
  src/DebugServer.cpp  src/DebugServer.h)

add_dependencies(brick brick_core_build)
target_include_directories(brick PRIVATE ${BRICK_ROOT}/core/include)
target_link_libraries(brick PRIVATE
  Qt6::Widgets Qt6::Network Qt6::WebEngineWidgets
  ${BRICK_CORE_LIB} pthread dl m)
target_compile_options(brick PRIVATE -Wall -Wextra)

# The app icon, built in so the window has it even when Brick isn't installed.
# PNGs rather than the SVG, so Qt's SVG module isn't needed.
qt_add_resources(brick "icon"
  PREFIX "/icon"
  BASE ${BRICK_ROOT}/icon/png
  FILES
    ${BRICK_ROOT}/icon/png/brick-16.png
    ${BRICK_ROOT}/icon/png/brick-32.png
    ${BRICK_ROOT}/icon/png/brick-48.png
    ${BRICK_ROOT}/icon/png/brick-64.png
    ${BRICK_ROOT}/icon/png/brick-128.png
    ${BRICK_ROOT}/icon/png/brick-256.png)

install(TARGETS brick RUNTIME DESTINATION bin)
install(FILES brick.desktop DESTINATION share/applications)
install(FILES ${BRICK_ROOT}/icon/brick.svg DESTINATION share/icons/hicolor/scalable/apps)
install(FILES ${BRICK_ROOT}/icon/png/brick-256.png DESTINATION share/icons/hicolor/256x256/apps RENAME brick.png)

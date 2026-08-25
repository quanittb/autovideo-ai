# Phase 20C Video Benchmark Manifest

## Shared Reference Identity

shared_reference_face: test-assets/phase20b/faces/face.jpg
description: Shared reference portrait identity for all REFERENCE_FACE_REPLACE benchmark cases.
sha256: 48747DB972E0A7C3CC3517F24EF5A730136B280FCD46BDAA70D502A3D849C31E
file_size: 842420
width: 1200
height: 1600
format: jpeg

---

## Physical Benchmark Videos

### C1 Physical Asset
video_file: test-assets/phase20c/videos/flow_acceptance_01.mp4
description: 1 person, clear face, low motion
sha256: 68747585122B46F78168F951AA43E461DBAFE19E4DFBA6D519578A004F8D1694
file_size: 2554476
container_duration: 9.988753
video_stream_duration: 9.988753
audio_stream_duration: 9.988753
width: 576
height: 1024
orientation: PORTRAIT / 9:16
fps: 30.00
frame_count: 299
video_codec: h264
audio_codec: aac
visible_face_count: 1
target_face_index: 0
replace_count: 1

### C2 Physical Asset
video_file: test-assets/phase20c/videos/flow_acceptance_02.mp4
description: 1 person, head rotation / expressions / light occlusion
sha256: 2832B907BDDE50A875CC6A784E3505A3E545885B3D8AEFCB0238947A302A8D91
file_size: 6743281
container_duration: 9.685313
video_stream_duration: 9.682000
audio_stream_duration: 9.685313
width: 1080
height: 1920
orientation: PORTRAIT / 9:16
fps: 30.00
frame_count: 291
video_codec: h264
audio_codec: aac
visible_face_count: 1
target_face_index: 0
replace_count: 1

### C3 Physical Asset
video_file: test-assets/phase20c/videos/flow_acceptance_03.mp4
description: multiple visible people, replace exactly ONE selected target face (Passenger / Right)
sha256: C2D030FCE3788E29C808B117A087F239D1E4B92B583EA9999CAF5191F76838DA
file_size: 5567429
container_duration: 9.898667
video_stream_duration: 9.898667
audio_stream_duration: 9.898667
width: 1080
height: 1920
orientation: PORTRAIT / 9:16
fps: 30.00
frame_count: 297
video_codec: h264
audio_codec: aac
visible_face_count: 2
target_face_index: 1
target_face_confirmed: YES
target_face_descriptor: PASSENGER_RIGHT
target_description: Right passenger seat, black jacket, holding mobile phone, front / 3-quarter view, talking to driver.
anchor_frame_timestamp_sec: 2.0
normalized_bounding_box: [0.5370, 0.4479, 0.2222, 0.1458]
replace_count: 1
preserve_non_target_faces: true

---

## Logical Benchmark Cases

### C1_GENERATED
case_id: C1_GENERATED
video_file: test-assets/phase20c/videos/flow_acceptance_01.mp4
transformation_intent: FACE_REPLACE
identity_mode: GENERATED
reference_face_file: null
target_face_index: 0
replace_count: 1
preserve_non_target_faces: true

### C1_REFERENCE
case_id: C1_REFERENCE
video_file: test-assets/phase20c/videos/flow_acceptance_01.mp4
transformation_intent: FACE_REPLACE
identity_mode: REFERENCE
reference_face_file: test-assets/phase20b/faces/face.jpg
target_face_index: 0
replace_count: 1
preserve_non_target_faces: true

### C2_GENERATED
case_id: C2_GENERATED
video_file: test-assets/phase20c/videos/flow_acceptance_02.mp4
transformation_intent: FACE_REPLACE
identity_mode: GENERATED
reference_face_file: null
target_face_index: 0
replace_count: 1
preserve_non_target_faces: true

### C2_REFERENCE
case_id: C2_REFERENCE
video_file: test-assets/phase20c/videos/flow_acceptance_02.mp4
transformation_intent: FACE_REPLACE
identity_mode: REFERENCE
reference_face_file: test-assets/phase20b/faces/face.jpg
target_face_index: 0
replace_count: 1
preserve_non_target_faces: true

### C3_GENERATED
case_id: C3_GENERATED
video_file: test-assets/phase20c/videos/flow_acceptance_03.mp4
transformation_intent: FACE_REPLACE
identity_mode: GENERATED
reference_face_file: null
target_face_index: 1
target_face_confirmed: YES
target_face_descriptor: PASSENGER_RIGHT
anchor_frame_timestamp_sec: 2.0
normalized_bounding_box: [0.5370, 0.4479, 0.2222, 0.1458]
replace_count: 1
preserve_non_target_faces: true

### C3_REFERENCE
case_id: C3_REFERENCE
video_file: test-assets/phase20c/videos/flow_acceptance_03.mp4
transformation_intent: FACE_REPLACE
identity_mode: REFERENCE
reference_face_file: test-assets/phase20b/faces/face.jpg
target_face_index: 1
target_face_confirmed: YES
target_face_descriptor: PASSENGER_RIGHT
anchor_frame_timestamp_sec: 2.0
normalized_bounding_box: [0.5370, 0.4479, 0.2222, 0.1458]
replace_count: 1
preserve_non_target_faces: true

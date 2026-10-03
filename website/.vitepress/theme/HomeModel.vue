<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue';
import { withBase } from 'vitepress';

const MODEL = '/models/windmill.glb';
const DRACO = '/draco/';
const PAPER = 0xf6f7f9;
const RADIUS = 16;
const DRIFT_SECONDS = 40;
const DRIFT_ARC = Math.PI / 18;
const BASE_ANGLE = Math.PI * 0.15;
const SHIFT = 0.12;

const host = ref<HTMLDivElement | null>(null);
const ready = ref(false);
let teardown: (() => void) | undefined;

onMounted(async () => {
  const el = host.value;
  if (!el) {
    return;
  }

  const THREE = await import('three');
  const { GLTFLoader } = await import('three/addons/loaders/GLTFLoader.js');
  const { DRACOLoader } = await import('three/addons/loaders/DRACOLoader.js');
  if (!host.value) {
    return;
  }

  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, powerPreference: 'low-power' });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.5));
  renderer.setClearColor(0x000000, 0);
  renderer.toneMapping = THREE.ACESFilmicToneMapping;
  renderer.toneMappingExposure = 1;
  el.appendChild(renderer.domElement);

  const scene = new THREE.Scene();
  scene.fog = new THREE.Fog(PAPER, RADIUS * 1.6, RADIUS * 4.2);
  scene.add(new THREE.HemisphereLight('#ffffff', '#c9c4b8', 0.9));
  const sun = new THREE.DirectionalLight('#fff4e6', 1.6);
  sun.position.set(RADIUS, RADIUS * 0.9, RADIUS * 0.4);
  scene.add(sun);

  const camera = new THREE.PerspectiveCamera(30, 1, 0.5, 200);

  const resize = () => {
    const { width, height } = el.getBoundingClientRect();
    if (width === 0 || height === 0) {
      return;
    }
    renderer.setSize(width, height, false);
    camera.aspect = width / height;
    if (width >= 960) {
      camera.setViewOffset(width, height, -width * SHIFT, 0, width, height);
    } else {
      camera.clearViewOffset();
    }
    camera.updateProjectionMatrix();
  };
  const observer = new ResizeObserver(resize);
  observer.observe(el);
  resize();

  const draco = new DRACOLoader().setDecoderPath(withBase(DRACO)).setDecoderConfig({ type: 'wasm' });
  const loader = new GLTFLoader().setDRACOLoader(draco);
  let model: InstanceType<typeof THREE.Object3D> | undefined;
  loader.load(withBase(MODEL), (gltf) => {
    model = gltf.scene;
    model.updateMatrixWorld(true);
    const box = new THREE.Box3().setFromObject(model);
    const center = box.getCenter(new THREE.Vector3());
    model.position.set(-center.x, -box.min.y, -center.z);
    scene.add(model);
    ready.value = true;
  });

  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)');
  const clock = new THREE.Clock();
  let t = 0;
  let frame = 0;
  const tick = () => {
    frame = requestAnimationFrame(tick);
    const dt = clock.getDelta();
    if (!reduced.matches) {
      t += dt;
    }
    const a = BASE_ANGLE + Math.sin((t / DRIFT_SECONDS) * Math.PI * 2) * DRIFT_ARC;
    const r = RADIUS * 2.15;
    camera.position.set(Math.sin(a) * r, RADIUS * 0.42, Math.cos(a) * r);
    camera.lookAt(0, RADIUS * 0.55, 0);
    renderer.render(scene, camera);
  };
  tick();

  teardown = () => {
    cancelAnimationFrame(frame);
    observer.disconnect();
    draco.dispose();
    model?.traverse((node) => {
      const mesh = node as InstanceType<typeof THREE.Mesh>;
      if (!mesh.isMesh) {
        return;
      }
      mesh.geometry.dispose();
      for (const material of [mesh.material].flat()) {
        material.dispose();
      }
    });
    renderer.dispose();
    renderer.domElement.remove();
  };
});

onBeforeUnmount(() => teardown?.());
</script>

<template>
  <div ref="host" class="ws-model" :class="{ ready }" aria-hidden="true" />
</template>

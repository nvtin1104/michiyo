<script lang="ts">
  import Mascot from './Mascot.svelte';
  import type { Pose } from './machine';
  export let state: Pose = 'idle';
  export let speed = 1;
  export let reduced = false;
  export let skin = 'default';
  export let facing = 1;
  export let motionDistance: number | undefined = undefined;
  export let pointer = { x: 80, y: 80 };
  export let tracking = true;
  export let eyeTracking = false;
</script>

<Mascot
  {state}
  {facing}
  {motionDistance}
  {speed}
  {reduced}
  {pointer}
  {tracking}
  {eyeTracking}
  {skin}
/>

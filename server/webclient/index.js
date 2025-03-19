
function submitMultipart() {
  console.log("Calling event listener of mp submit");
  const fileIpt = document.getElementById("mp-file");
  const receiverIpt = document.getElementById("mp-receiver");
  const durationIpt = document.getElementById("mp-duration");

  const formData = new FormData();
  const file = fileIpt.files[0];
  formData.append("image", file);
  formData.append("receiver", receiverIpt.value);
  formData.append("duration", durationIpt.value * 60 * 60);

  const xhr = new XMLHttpRequest();
  xhr.open("POST", "/mp_new_image_message", true);

  xhr.onload = () => {
    console.log("Finished sending form data.");
  }
  xhr.send(formData);
}


window.addEventListener("load", () => {
  console.log("Loaded document");
  const submitBtn = document.getElementById("mp-submit");
  if (submitBtn) {
    console.log("Adding event listener to mp submit");
    submitBtn.addEventListener("click", submitMultipart);
  }
});
